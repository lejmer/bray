use bray_bound_tree::{
    BoundAnonymousCallableExpression, BoundArgument, BoundAssignmentExpression,
    BoundBinaryExpression, BoundBlockExpression, BoundCallExpression, BoundConversionExpression,
    BoundErrorExpression, BoundExpression, BoundExpressionId, BoundNameExpression, BoundOperator,
    BoundStructuredExpression, BoundStructuredExpressionKind, BoundUnaryExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeId, MemberLookupResult, TypeId};
use bray_syntax::{
    AccessExpressionSyntax, CallOperationSyntax, ConversionOperationSyntax, ExpressionSyntax,
    ForExpressionSyntax, LambdaExpressionSyntax, MatchExpressionSyntax, PrimaryExpressionSyntax,
    SourceSyntaxNode, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    TypeExpressionSyntax, walk_syntax_node,
};

use super::super::block::BlockBindingOperations;
use super::super::name::symbol_name;
use super::super::{BindingError, BindingResult};
use super::support::{
    classify_operator, control_transfer_target, member_selector, structured_kind, value_target,
    visit_direct_nodes,
};
use crate::BinderFactContext;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::request::{BinderRequestContext, ControlTarget, ControlTargetKind};

pub(crate) struct ExpressionBinder {
    pub(super) path_context: PathBindingContext,
    pub(super) error_type: TypeId,
}

impl ExpressionBinder {
    pub(crate) const fn new(path_context: PathBindingContext, error_type: TypeId) -> Self {
        Self {
            path_context,
            error_type,
        }
    }

    pub(crate) fn bind_expression<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        request.check_cancellation()?;

        let Some(syntax) = syntax else {
            return self.push_missing_error(request);
        };

        let expression = if let Some(operator) = syntax.operator_token() {
            self.bind_operator(request, scope, syntax, operator.kind())?
        } else if let Some(primary) = syntax.primary_expression() {
            self.bind_primary(request, scope, &primary)?
        } else if let Some(base) = syntax.expressions().next() {
            self.bind_expression(request, scope, Some(&base))?
        } else {
            return self.push_error(request, Some(syntax));
        };

        self.bind_postfixes(request, scope, syntax, expression)
    }

    fn bind_operator<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &ExpressionSyntax,
        operator_kind: SyntaxKind,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let operator = classify_operator(operator_kind).ok_or(BindingError::UnsupportedSyntax)?;
        let mut operands = Vec::new();

        for operand in syntax.expressions() {
            operands.push(self.bind_expression(request, scope, Some(&operand))?);
        }

        let origin = request.source_origin(syntax);
        let recovered = syntax.is_recovered()
            || operands.len() > 2
            || operands
                .iter()
                .any(|operand| request.expression_is_recovered(*operand));

        let expression = if operator == BoundOperator::Assign {
            BoundExpression::Assignment(BoundAssignmentExpression::new(
                origin,
                operator,
                operands,
                self.error_type,
                recovered,
            ))
        } else if operands.len() == 1 {
            BoundExpression::Unary(BoundUnaryExpression::new(
                origin,
                operator,
                operands,
                self.error_type,
                recovered,
            ))
        } else {
            BoundExpression::Binary(BoundBinaryExpression::new(
                origin,
                operator,
                operands,
                self.error_type,
                recovered,
            ))
        };

        self.push(request, expression)
    }

    fn bind_primary<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &PrimaryExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        if let Some(access) = syntax.access_expression() {
            return self.bind_access(request, scope, &access);
        }

        if let Some(block) = syntax.block_expression() {
            let block_id = request.bind_block(scope, &block, self)?;
            let recovered = block.is_recovered();

            return self.push(
                request,
                BoundExpression::Block(BoundBlockExpression::new(
                    request.source_origin(&block),
                    block_id,
                    self.error_type,
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
                    self.error_type,
                    lambda.is_recovered(),
                )),
            );
        }

        if root.kind() == SyntaxKind::GroupedExpression {
            return self.bind_first_descendant_expression(request, scope, root, recovery_origin);
        }

        let Some(kind) = structured_kind(root.kind()) else {
            return self.push_error(request, Some(recovery_origin));
        };

        self.bind_structured(request, scope, root, kind)
    }

    fn bind_access<C>(
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
                let target = request.bind_value_identifier(
                    self.path_context_for(scope, access),
                    syntax.source(),
                    token,
                );

                match target {
                    MemberLookupResult::Found(target) => self.push(
                        request,
                        BoundExpression::Name(BoundNameExpression::new(
                            request.source_origin(syntax),
                            value_target(target),
                            self.error_type,
                            syntax.is_recovered(),
                        )),
                    )?,
                    _ => self.push_error(request, Some(syntax))?,
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
            let kind = match operation.kind() {
                SyntaxKind::MemberAccessOperation => BoundStructuredExpressionKind::MemberAccess,
                SyntaxKind::ElementIndexOperation => BoundStructuredExpressionKind::ElementIndex,
                _ => return SyntaxWalkControl::Continue,
            };

            match self.bind_structured_with_operand(request, scope, operation, kind, current) {
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

    fn bind_postfixes<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &ExpressionSyntax,
        mut current: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut failure = None;

        visit_direct_nodes(syntax, |operation| {
            let result = match operation.kind() {
                SyntaxKind::CallOperation => {
                    let Some(call) = operation.cast::<CallOperationSyntax>() else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_call(request, scope, &call, current)
                }
                SyntaxKind::ConversionOperation => {
                    let Some(conversion) = operation.cast::<ConversionOperationSyntax>() else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_conversion(request, &conversion, current)
                }
                SyntaxKind::MemberAccessOperation => self.bind_structured_with_operand(
                    request,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::MemberAccess,
                    current,
                ),
                SyntaxKind::ElementIndexOperation => self.bind_structured_with_operand(
                    request,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::ElementIndex,
                    current,
                ),
                SyntaxKind::SliceIndexOperation => self.bind_structured_with_operand(
                    request,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::SliceIndex,
                    current,
                ),
                SyntaxKind::NullablePropagationOperation => self.bind_structured_with_operand(
                    request,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::NullablePropagation,
                    current,
                ),
                SyntaxKind::TraitQualifiedMemberOperation => self.bind_structured_with_operand(
                    request,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::TraitQualifiedMember,
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

    fn bind_call<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &CallOperationSyntax,
        callee: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut arguments = Vec::new();
        let list = syntax.argument_list();

        for argument in list.arguments() {
            let expression = self.bind_expression(request, scope, Some(&argument.expression()))?;
            let name = argument
                .identifier_token()
                .and_then(|token| symbol_name(argument.source(), &token));

            arguments.push(BoundArgument::new(
                expression,
                name,
                argument.is_recovered(),
            ));
        }

        let recovered = syntax.is_recovered()
            || arguments.iter().any(|argument| argument.is_recovered())
            || request.expression_is_recovered(callee);

        self.push(
            request,
            BoundExpression::Call(BoundCallExpression::new(
                request.source_origin(syntax),
                callee,
                arguments,
                self.error_type,
                recovered,
            )),
        )
    }

    fn bind_conversion<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        syntax: &ConversionOperationSyntax,
        operand: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let recovered = syntax.is_recovered() || request.expression_is_recovered(operand);

        self.push(
            request,
            BoundExpression::Conversion(BoundConversionExpression::new(
                request.source_origin(syntax),
                operand,
                SyntaxAnchor::from_node(&syntax.type_expression()),
                None,
                self.error_type,
                recovered,
            )),
        )
    }

    fn bind_structured<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
        kind: BoundStructuredExpressionKind,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let loop_target = matches!(
            kind,
            BoundStructuredExpressionKind::While
                | BoundStructuredExpressionKind::For
                | BoundStructuredExpressionKind::Loop
        )
        .then(|| {
            ControlTarget::new(
                ControlTargetKind::Loop,
                SyntaxAnchor::from_node(&syntax),
                None,
            )
        });

        if let Some(target) = loop_target {
            request.push_control_target(target);
        }

        let children = self.bind_semantic_children(request, scope, syntax);

        if let Some(target) = loop_target
            && request.pop_control_target() != Some(target)
        {
            return Err(BindingError::ControlTargetMismatch);
        }

        let (operands, blocks) = children?;
        let recovered = syntax.is_recovered()
            || operands
                .iter()
                .any(|operand| request.expression_is_recovered(*operand))
            || blocks
                .iter()
                .any(|block| request.block_is_recovered(*block));

        let expression = BoundStructuredExpression::new(
            request.source_origin(&syntax),
            kind,
            operands,
            blocks,
            [],
            self.error_type,
            recovered,
        )
        .with_member_selector(member_selector(syntax))
        .with_control_target(control_transfer_target(request, kind));

        self.push(request, BoundExpression::Structured(expression))
    }

    fn bind_structured_with_operand<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
        kind: BoundStructuredExpressionKind,
        operand: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let (children, blocks) = self.bind_semantic_children(request, scope, syntax)?;
        let recovered = syntax.is_recovered()
            || request.expression_is_recovered(operand)
            || children
                .iter()
                .any(|child| request.expression_is_recovered(*child))
            || blocks
                .iter()
                .any(|block| request.block_is_recovered(*block));

        let operands = std::iter::once(operand).chain(children);

        let expression = BoundStructuredExpression::new(
            request.source_origin(&syntax),
            kind,
            operands,
            blocks,
            [],
            self.error_type,
            recovered,
        )
        .with_member_selector(member_selector(syntax))
        .with_control_target(control_transfer_target(request, kind));

        self.push(request, BoundExpression::Structured(expression))
    }

    fn bind_semantic_children<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
    ) -> BindingResult<(Vec<BoundExpressionId>, Vec<bray_bound_tree::BoundBlockId>)>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut operands = Vec::new();
        let mut blocks = Vec::new();
        let mut first = true;
        let mut failure = None;

        walk_syntax_node(&syntax, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            if first {
                first = false;

                return SyntaxWalkControl::Continue;
            }

            if node.kind() == SyntaxKind::Expression {
                let Some(expression) = node.cast::<ExpressionSyntax>() else {
                    failure = Some(BindingError::UnsupportedSyntax);

                    return SyntaxWalkControl::Stop;
                };

                match self.bind_expression(request, scope, Some(&expression)) {
                    Ok(expression) => operands.push(expression),
                    Err(error) => {
                        failure = Some(error);

                        return SyntaxWalkControl::Stop;
                    }
                }

                return SyntaxWalkControl::SkipChildren;
            }

            if node.kind() == SyntaxKind::BlockExpression {
                let Some(block) = node.cast::<bray_syntax::BlockExpressionSyntax>() else {
                    failure = Some(BindingError::UnsupportedSyntax);

                    return SyntaxWalkControl::Stop;
                };

                match request.bind_block(scope, &block, self) {
                    Ok(block) => blocks.push(block),
                    Err(error) => {
                        failure = Some(error);

                        return SyntaxWalkControl::Stop;
                    }
                }

                return SyntaxWalkControl::SkipChildren;
            }

            SyntaxWalkControl::Continue
        });

        match failure {
            Some(error) => Err(error),
            None => Ok((operands, blocks)),
        }
    }

    fn bind_first_descendant_expression<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
        recovery_origin: &PrimaryExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut result = None;
        let mut first = true;

        walk_syntax_node(&syntax, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            if first {
                first = false;

                return SyntaxWalkControl::Continue;
            }

            if node.kind() != SyntaxKind::Expression {
                return SyntaxWalkControl::Continue;
            }

            let Some(expression) = node.cast::<ExpressionSyntax>() else {
                result = Some(Err(BindingError::UnsupportedSyntax));

                return SyntaxWalkControl::Stop;
            };

            result = Some(self.bind_expression(request, scope, Some(&expression)));

            SyntaxWalkControl::Stop
        });

        match result {
            Some(result) => result,
            None => self.push_error(request, Some(recovery_origin)),
        }
    }

    pub(super) fn path_context_for(
        &self,
        scope: LocalScopeId,
        access: NameAccess,
    ) -> PathBindingContext {
        PathBindingContext::new(
            scope,
            self.path_context.module(),
            self.path_context.module_owner(),
            access,
        )
    }

    pub(super) fn push<C>(
        &self,
        request: &mut BinderRequestContext<'_, C>,
        expression: BoundExpression,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        request
            .unit_mut()
            .tree_mut()
            .push_expression(expression)
            .map_err(crate::unit::BoundUnitConstructionError::from)
            .map_err(Into::into)
    }

    fn push_error<C>(
        &self,
        request: &mut BinderRequestContext<'_, C>,
        syntax: Option<&impl SourceSyntaxNode>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let origin = syntax.map_or_else(
            || bray_bound_tree::BoundNodeOrigin::source(request.unit().key().source()),
            |syntax| request.source_origin(syntax),
        );

        self.push(
            request,
            BoundExpression::Error(BoundErrorExpression::new(origin, self.error_type)),
        )
    }

    fn push_missing_error<C>(
        &self,
        request: &mut BinderRequestContext<'_, C>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let origin = bray_bound_tree::BoundNodeOrigin::source(request.unit().key().source());

        self.push(
            request,
            BoundExpression::Error(BoundErrorExpression::new(origin, self.error_type)),
        )
    }
}

impl<C> BlockBindingOperations<C> for ExpressionBinder
where
    C: BinderFactContext + ?Sized,
{
    fn bind_expression(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<BoundExpressionId> {
        ExpressionBinder::bind_expression(self, request, scope, syntax)
    }

    fn bind_generator_expression(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &bray_syntax::GeneratorIterationExpressionSyntax,
    ) -> BindingResult<BoundExpressionId> {
        let mut result = None;

        visit_direct_nodes(syntax, |node| {
            if let Some(expression) = node.cast::<ExpressionSyntax>() {
                result = Some(self.bind_expression(request, scope, Some(&expression)));
            }

            SyntaxWalkControl::Stop
        });

        match result {
            Some(result) => result,
            None => self.push_error(request, Some(syntax)),
        }
    }

    fn bind_type_expression(
        &mut self,
        _request: &mut BinderRequestContext<'_, C>,
        _scope: LocalScopeId,
        _syntax: Option<&TypeExpressionSyntax>,
    ) -> BindingResult<Option<TypeId>> {
        Ok(None)
    }

    fn error_type(&self) -> TypeId {
        self.error_type
    }

    fn path_context(
        &self,
        _request: &BinderRequestContext<'_, C>,
        scope: LocalScopeId,
    ) -> BindingResult<PathBindingContext> {
        Ok(self.path_context_for(scope, self.path_context.access()))
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundExpression, BoundOperator, BoundStructuredExpressionKind, BoundWalkControl,
        BoundWalkEvent, walk_bound_tree,
    };
    use bray_symbols::SymbolOrigin;

    use crate::BinderFactContext;
    use crate::fact::test_support::TestFixture;
    use crate::lookup::{NameAccess, PathBindingContext};

    #[test]
    fn expression_blocks_bind_names_operators_calls_and_control_flow() {
        let fixture = TestFixture::from_source(
            "module app; const Size: Int = 1; func main() { Size + Size; Size(value = Size); Size.field; Size as Int; if Size {}; for item in mut Size { yield item; } else { yield none; }; match Size { case Size { yield Size; } }; loop { break; }; lambda(value: Int) {}; return Size; }",
        );

        let facts = fixture.context();
        let (mut request, syntax) = crate::binding::test_support::request_and_block(&facts);
        let root_scope = request.unit().root_scope();

        let path_context = path_context(&facts, root_scope);

        let block = match request.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("valid expression block must bind: {error:?}"),
        };

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("bound expression block must freeze: {error:?}"),
        };

        assert!(
            result.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics()
        );

        let mut saw_add = false;
        let mut saw_call = false;
        let mut saw_conversion = false;
        let mut saw_named_member = false;
        let mut saw_conditional = false;
        let mut saw_for_pattern = false;
        let mut saw_match_pattern = false;
        let mut saw_targeted_break = false;
        let mut saw_targeted_return = false;
        let mut saw_lambda = false;

        walk_bound_tree(result.unit().tree(), block, |event| {
            let BoundWalkEvent::Enter(node) = event else {
                return BoundWalkControl::Continue;
            };

            let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
                return BoundWalkControl::Continue;
            };

            let Some(expression) = result.unit().tree().expression(expression) else {
                return BoundWalkControl::Continue;
            };

            match expression {
                BoundExpression::Binary(expression)
                    if expression.operator() == BoundOperator::Add =>
                {
                    saw_add = true;
                }
                BoundExpression::Call(expression)
                    if expression.arguments().len() == 1
                        && expression.arguments()[0]
                            .name()
                            .is_some_and(|name| name.as_str() == "value") =>
                {
                    saw_call = true;
                }
                BoundExpression::Conversion(expression)
                    if expression.target_syntax().syntax_kind()
                        == bray_syntax::SyntaxKind::TypeExpression =>
                {
                    saw_conversion = true;
                }
                BoundExpression::Structured(expression)
                    if matches!(
                        expression.member_selector(),
                        Some(bray_bound_tree::BoundMemberSelector::Name(name))
                            if name.as_str() == "field"
                    ) =>
                {
                    saw_named_member = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Conditional =>
                {
                    saw_conditional = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::For
                        && expression.patterns().len() == 1 =>
                {
                    saw_for_pattern = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Match
                        && expression.patterns().len() == 1 =>
                {
                    saw_match_pattern = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Break
                        && expression.control_target().is_some() =>
                {
                    saw_targeted_break = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Return
                        && expression.control_target().is_some() =>
                {
                    saw_targeted_return = true;
                }
                BoundExpression::AnonymousCallable(_) => saw_lambda = true,
                _ => {}
            }

            BoundWalkControl::Continue
        });

        assert!(saw_add);
        assert!(saw_call);
        assert!(saw_conversion);
        assert!(saw_named_member);
        assert!(saw_conditional);
        assert!(saw_for_pattern);
        assert!(saw_match_pattern);
        assert!(saw_targeted_break);
        assert!(saw_targeted_return);
        assert!(saw_lambda);
        assert_eq!(result.dependencies().len(), 1);
    }

    #[test]
    fn unresolved_expressions_recover_without_losing_the_enclosing_block() {
        let fixture =
            TestFixture::from_source("module app; const Size: Int = 1; func main() { Missing; }");

        let facts = fixture.context();
        let (mut request, syntax) = crate::binding::test_support::request_and_block(&facts);
        let root_scope = request.unit().root_scope();
        let path_context = path_context(&facts, root_scope);

        let block = match request.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("malformed expression block must recover: {error:?}"),
        };

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("recovered expression block must freeze: {error:?}"),
        };

        assert_eq!(result.diagnostics().len(), 1);

        let Some(block) = result.unit().tree().block(block) else {
            panic!("recovered block must remain in the tree");
        };

        assert!(block.is_recovered());

        let [bray_bound_tree::BoundBlockItem::Expression(expression)] = block.items() else {
            panic!("recovered block must retain its expression item");
        };

        assert!(matches!(
            result.unit().tree().expression(*expression),
            Some(BoundExpression::Error(_))
        ));
    }

    fn path_context<C: BinderFactContext + ?Sized>(
        facts: &C,
        scope: bray_symbols::LocalScopeId,
    ) -> PathBindingContext {
        let Some(module) = facts
            .symbols()
            .modules()
            .iter()
            .find(|module| module.origin() == SymbolOrigin::Source)
        else {
            panic!("test graph must contain a source module");
        };

        PathBindingContext::new(scope, module.id(), module.owner(), NameAccess::Internal)
    }
}
