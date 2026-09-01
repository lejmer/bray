use bray_bound_tree::{
    BoundAnonymousCallableExpression, BoundBlockExpression, BoundExpression, BoundExpressionId,
    BoundNameExpression, BoundPatternReferenceExpression, BoundStructuredExpressionKind,
    BoundUnqualifiedVariantExpression, BoundUnresolvedReferenceExpression,
};
use bray_symbols::{LocalScopeId, SymbolName};
use bray_syntax::{
    AccessExpressionSyntax, ArrayExpressionSyntax, AwaitExpressionSyntax,
    BooleanFoldExpressionSyntax, ForExpressionSyntax, GeneralGeneratorExpressionSyntax,
    LambdaExpressionSyntax, LeadingDotVariantExpressionSyntax, LiteralExpressionSyntax,
    MatchExpressionSyntax, PrimaryExpressionSyntax, SourceSyntaxNode, SyntaxKind, SyntaxNodeView,
    SyntaxWalkControl, TypeExpressionSyntax, WithExpressionSyntax, walk_direct_child_nodes,
};

use super::super::{BindingError, BindingResult};
use super::ExpressionBinder;
use super::core::UnresolvedNameBinding;
use super::support::{ReferenceResolution, classify_reference_result, structured_kind};
use crate::BindingQueryContext;
use crate::binder::Binder;
use crate::lookup::{NameAccess, ResolvedValueName};

impl ExpressionBinder {
    pub(crate) fn bind_type_expression_value<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &TypeExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let Some(path) = syntax.path() else {
            return self.push_error(binder, Some(syntax));
        };

        let result = binder.bind_value_path(
            self.path_context_for(scope, self.path_context.access()),
            &path,
        )?;

        let resolution = classify_reference_result(
            result.map(ResolvedValueName::into_name, |candidate| candidate),
        );

        match resolution {
            ReferenceResolution::Resolved(target) => {
                self.push_resolved_reference(binder, syntax, target)
            }
            ReferenceResolution::Unresolved(kind, candidates) => self.push(
                binder,
                BoundExpression::UnresolvedReference(BoundUnresolvedReferenceExpression::new(
                    binder.source_origin(syntax),
                    kind,
                    candidates,
                    self.error_type,
                )),
            ),
        }
    }

    pub(super) fn bind_primary<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &PrimaryExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
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
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
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

            return self.bind_generator_region(
                binder,
                scope,
                &generator,
                &generator.generator_iteration_expression(),
                BoundStructuredExpressionKind::GeneralGenerator,
            );
        }

        if root.kind() == SyntaxKind::ForExpression {
            let Some(expression) = root.cast::<ForExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_for_expression(binder, scope, &expression);
        }

        if root.kind() == SyntaxKind::WithExpression {
            let Some(expression) = root.cast::<WithExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_with_expression(binder, scope, &expression);
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

        if root.kind() == SyntaxKind::LiteralExpression {
            let Some(literal) = root.cast::<LiteralExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            return self.bind_literal(binder, &literal);
        }

        if root.kind() == SyntaxKind::ArrayExpression {
            let Some(array) = root.cast::<ArrayExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            if let Some(iteration) = array.generator_iteration_expressions().next() {
                return self.bind_generator_region(
                    binder,
                    scope,
                    &array,
                    &iteration,
                    BoundStructuredExpressionKind::ArrayGenerator,
                );
            }

            let kind = if array.semicolon_token().is_some() {
                BoundStructuredExpressionKind::RepeatedArray
            } else {
                BoundStructuredExpressionKind::Array
            };

            return self.bind_structured(binder, scope, root, kind);
        }

        if root.kind() == SyntaxKind::BooleanFoldExpression {
            let Some(expression) = root.cast::<BooleanFoldExpressionSyntax>() else {
                return self.push_error(binder, Some(recovery_origin));
            };

            let kind = if expression.any_keyword().is_some() {
                BoundStructuredExpressionKind::BooleanAnyFold
            } else {
                BoundStructuredExpressionKind::BooleanAllFold
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
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        self.bind_access_with_access(binder, scope, syntax, self.path_context.access())
    }

    fn bind_access_with_access<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &AccessExpressionSyntax,
        access: NameAccess,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let root_token = syntax.identifier_token().or_else(|| syntax.self_token());

        let mut route_tokens = root_token.iter().cloned().collect::<Vec<_>>();
        let mut collecting_route = root_token.is_some();

        walk_direct_child_nodes(syntax, |operation| {
            if !collecting_route || operation.kind() != SyntaxKind::MemberAccessOperation {
                collecting_route = false;

                return SyntaxWalkControl::Continue;
            }

            let Some(member) = operation.cast::<bray_syntax::MemberAccessOperationSyntax>() else {
                collecting_route = false;

                return SyntaxWalkControl::Continue;
            };

            let Some(token) = member.identifier_token() else {
                collecting_route = false;

                return SyntaxWalkControl::Continue;
            };

            route_tokens.push(token);

            SyntaxWalkControl::Continue
        });

        let context = self.path_context_for(scope, access);

        let route = if let Some(token) = root_token.as_ref() {
            let root =
                binder.lookup_reference_identifier(context, syntax.source(), token.clone())?;

            if matches!(root, bray_symbols::MemberLookupResult::NotFound) {
                binder.lookup_module_route(context, syntax.source(), route_tokens)?
            } else {
                None
            }
        } else {
            None
        };

        let mut current = match (route, root_token) {
            (Some((module, _)), _) => self.push_resolved_reference(
                binder,
                syntax,
                bray_bound_tree::BoundReferenceTarget::Surface(module.into()),
            )?,
            (None, Some(token)) => {
                self.bind_root_reference(binder, scope, syntax, token, access)?
            }
            (None, None) => match syntax.access_expressions().next() {
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
        let mut member_index = 0_usize;

        walk_direct_child_nodes(syntax, |operation| {
            let result = match operation.kind() {
                SyntaxKind::MemberAccessOperation => {
                    member_index += 1;

                    if route.is_some_and(|(_, length)| member_index < length) {
                        return SyntaxWalkControl::Continue;
                    }

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

    fn bind_root_reference<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &AccessExpressionSyntax,
        token: bray_syntax::SyntaxToken,
        access: NameAccess,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let name = token
            .text(syntax.source().text())
            .and_then(SymbolName::try_new);

        let context = self.path_context_for(scope, access);

        let contextual_pattern = match name.as_ref() {
            Some(name) => binder.contextual_pattern_binding(scope, name.as_str())?,
            None => None,
        };

        if let Some((binding, pattern)) = contextual_pattern {
            let Some(name) = name else {
                return self.push_error(binder, Some(syntax));
            };

            let resolution = classify_reference_result(binder.lookup_reference_identifier(
                context,
                syntax.source(),
                token,
            )?);

            return match resolution {
                ReferenceResolution::Unresolved(kind, candidates) => self.push(
                    binder,
                    BoundExpression::PatternReference(BoundPatternReferenceExpression::new(
                        binder.source_origin(syntax),
                        pattern,
                        binding,
                        name,
                        kind,
                        candidates,
                        syntax.is_recovered(),
                    )),
                ),
                ReferenceResolution::Resolved(target) => {
                    self.push_resolved_reference(binder, syntax, target)
                }
            };
        }

        let result = match self.unresolved_names {
            UnresolvedNameBinding::ContextualVariant => {
                binder.bind_contextual_variant_identifier(context, syntax.source(), token)?
            }
            UnresolvedNameBinding::Diagnostic => {
                binder.bind_reference_identifier(context, syntax.source(), token)?
            }
        };

        let resolution = classify_reference_result(result);

        match resolution {
            ReferenceResolution::Resolved(target) => {
                self.push_resolved_reference(binder, syntax, target)
            }
            ReferenceResolution::Unresolved(
                bray_bound_tree::BoundUnresolvedReferenceKind::NotFound,
                _,
            ) if name.is_some()
                && self.unresolved_names == UnresolvedNameBinding::ContextualVariant =>
            {
                let Some(name) = name else {
                    return self.push_error(binder, Some(syntax));
                };

                self.push(
                    binder,
                    BoundExpression::UnqualifiedVariant(BoundUnqualifiedVariantExpression::new(
                        binder.source_origin(syntax),
                        name,
                        None,
                        syntax.is_recovered(),
                    )),
                )
            }
            ReferenceResolution::Unresolved(kind, candidates) => self.push(
                binder,
                BoundExpression::UnresolvedReference(BoundUnresolvedReferenceExpression::new(
                    binder.source_origin(syntax),
                    kind,
                    candidates,
                    self.error_type,
                )),
            ),
        }
    }

    pub(in crate::binding::expression) fn push_resolved_reference<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        syntax: &impl SourceSyntaxNode,
        target: bray_bound_tree::BoundReferenceTarget,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let ty = binder.value_type(target);

        self.push(
            binder,
            BoundExpression::Name(BoundNameExpression::new(
                binder.source_origin(syntax),
                target,
                ty,
                syntax.is_recovered(),
            )),
        )
    }
}
