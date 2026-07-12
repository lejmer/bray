use bray_bound_tree::{
    BoundAnonymousCallableExpression, BoundBlockExpression, BoundExpression, BoundExpressionId,
    BoundNameExpression, BoundStructuredExpressionKind, BoundUnresolvedReferenceExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::LocalScopeId;
use bray_syntax::{
    AccessExpressionSyntax, ForExpressionSyntax, GeneralGeneratorExpressionSyntax,
    LambdaExpressionSyntax, LeadingDotVariantExpressionSyntax, MatchExpressionSyntax,
    PrimaryExpressionSyntax, SourceSyntaxNode, SpawnExpressionSyntax, SyntaxKind, SyntaxNodeView,
    SyntaxWalkControl,
};

use super::super::{BindingError, BindingResult};
use super::ExpressionBinder;
use super::support::{
    ReferenceResolution, classify_reference_result, structured_kind, visit_direct_nodes,
};
use crate::BinderFactContext;
use crate::lookup::NameAccess;
use crate::request::BinderRequestContext;

impl ExpressionBinder {
    pub(super) fn bind_primary<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &PrimaryExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        if let Some(access) = syntax.access_expression() {
            let head = self.bind_access(request, scope, &access)?;

            return match syntax.struct_construction_body() {
                Some(body) => self.bind_struct_construction(request, scope, &body, Some(head)),
                None => Ok(head),
            };
        }

        if let Some(body) = syntax.struct_construction_body() {
            return self.bind_struct_construction(request, scope, &body, None);
        }

        if let Some(block) = syntax.block_expression() {
            let block_id = request.bind_block(scope, &block, self)?;
            let recovered = block.is_recovered();

            return self.push(
                request,
                BoundExpression::Block(BoundBlockExpression::new(
                    request.source_origin(&block),
                    block_id,
                    None,
                    recovered,
                )),
            );
        }

        let mut result = None;

        visit_direct_nodes(syntax, |root| {
            result = Some(self.bind_primary_node(request, scope, root, syntax));

            SyntaxWalkControl::Stop
        });

        match result {
            Some(result) => result,
            None => self.push_error(request, Some(syntax)),
        }
    }

    fn bind_primary_node<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
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
            return self.bind_control_transfer(request, scope, root);
        }

        if root.kind() == SyntaxKind::GeneralGeneratorExpression {
            let Some(generator) = root.cast::<GeneralGeneratorExpressionSyntax>() else {
                return self.push_error(request, Some(recovery_origin));
            };

            return self.bind_generator_iteration(
                request,
                scope,
                &generator.generator_iteration_expression(),
                SyntaxAnchor::from_node(&generator),
            );
        }

        if root.kind() == SyntaxKind::SpawnExpression {
            let Some(spawn) = root.cast::<SpawnExpressionSyntax>() else {
                return self.push_error(request, Some(recovery_origin));
            };

            return self.bind_spawn(request, scope, &spawn);
        }

        if root.kind() == SyntaxKind::ForExpression {
            let Some(expression) = root.cast::<ForExpressionSyntax>() else {
                return self.push_error(request, Some(recovery_origin));
            };

            return self.bind_for_expression(request, scope, &expression);
        }

        if root.kind() == SyntaxKind::MatchExpression {
            let Some(expression) = root.cast::<MatchExpressionSyntax>() else {
                return self.push_error(request, Some(recovery_origin));
            };

            return self.bind_match_expression(request, scope, &expression);
        }

        if root.kind() == SyntaxKind::LambdaExpression {
            let Some(lambda) = root.cast::<LambdaExpressionSyntax>() else {
                return self.push_error(request, Some(recovery_origin));
            };

            let unit = request.bind_anonymous_callable_reference(&lambda)?;

            return self.push(
                request,
                BoundExpression::AnonymousCallable(BoundAnonymousCallableExpression::new(
                    request.source_origin(&lambda),
                    unit,
                    None,
                    lambda.is_recovered(),
                )),
            );
        }

        if root.kind() == SyntaxKind::LeadingDotVariantExpression {
            let Some(variant) = root.cast::<LeadingDotVariantExpressionSyntax>() else {
                return self.push_error(request, Some(recovery_origin));
            };

            return self.bind_leading_dot_variant(request, &variant);
        }

        if root.kind() == SyntaxKind::GroupedExpression {
            return self.bind_first_descendant_expression(request, scope, root, recovery_origin);
        }

        let Some(kind) = structured_kind(root.kind()) else {
            return self.push_error(request, Some(recovery_origin));
        };

        self.bind_structured(request, scope, root, kind)
    }

    pub(in crate::binding::expression) fn bind_access<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &AccessExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        self.bind_access_with_access(request, scope, syntax, self.path_context.access())
    }

    fn bind_access_with_access<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
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
                let target = classify_reference_result(request.bind_reference_identifier(
                    self.path_context_for(scope, access),
                    syntax.source(),
                    token,
                ));

                match target {
                    ReferenceResolution::Resolved(target) => self.push(
                        request,
                        BoundExpression::Name(BoundNameExpression::new(
                            request.source_origin(syntax),
                            target,
                            None,
                            syntax.is_recovered(),
                        )),
                    )?,
                    ReferenceResolution::Unresolved(kind, candidates) => self.push(
                        request,
                        BoundExpression::UnresolvedReference(
                            BoundUnresolvedReferenceExpression::new(
                                request.source_origin(syntax),
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

                    self.bind_access_with_access(request, scope, &nested, nested_access)?
                }
                None => self.push_error(request, Some(syntax))?,
            },
        };

        let mut failure = None;

        visit_direct_nodes(syntax, |operation| {
            let result = match operation.kind() {
                SyntaxKind::MemberAccessOperation => {
                    let Some(member) = operation.cast::<bray_syntax::MemberAccessOperationSyntax>()
                    else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_member_access(request, &member, current)
                }
                SyntaxKind::ElementIndexOperation => self.bind_structured_with_operand(
                    request,
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
