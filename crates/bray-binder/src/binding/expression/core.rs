use bray_bound_tree::{
    BoundAssignmentExpression, BoundBinaryExpression, BoundErrorExpression, BoundExpression,
    BoundExpressionId, BoundOperator, BoundSliceBounds, BoundStructuredExpression,
    BoundStructuredExpressionKind, BoundTypeReference, BoundUnaryExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{BorrowKind, LocalScopeId, TypeId};
use bray_syntax::{
    BorrowExpressionSyntax, ExpressionSyntax, PrimaryExpressionSyntax, SliceIndexOperationSyntax,
    SourceSyntaxNode, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    TypeExpressionSyntax, walk_syntax_node,
};

use super::super::block::BlockBindingOperations;
use super::super::{BindingError, BindingResult};
use super::support::classify_operator;
use crate::BinderFactContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind};
use crate::lookup::{NameAccess, PathBindingContext};

pub(crate) struct ExpressionBinder {
    pub(in crate::binding::expression) path_context: PathBindingContext,
    pub(in crate::binding::expression) error_type: TypeId,
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
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        binder.check_cancellation()?;

        let Some(syntax) = syntax else {
            return self.push_missing_error(binder);
        };

        let expression = if let Some(primary) = syntax.primary_expression() {
            self.bind_primary(binder, scope, &primary)?
        } else if let Some(operator) = syntax.operator_token() {
            self.bind_operator(binder, scope, syntax, operator.kind())?
        } else if let Some(base) = syntax.expressions().next() {
            self.bind_expression(binder, scope, Some(&base))?
        } else {
            return self.push_error(binder, Some(syntax));
        };

        self.bind_postfixes(binder, scope, syntax, expression)
    }

    fn bind_operator<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
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
            operands.push(self.bind_expression(binder, scope, Some(&operand))?);
        }

        let origin = binder.source_origin(syntax);

        let recovered = syntax.is_recovered()
            || operands.len() > 2
            || operands
                .iter()
                .any(|operand| binder.expression_is_recovered(*operand));

        let expression = if operator == BoundOperator::Assign {
            BoundExpression::Assignment(BoundAssignmentExpression::new(
                origin, operator, operands, None, recovered,
            ))
        } else if operands.len() == 1 {
            BoundExpression::Unary(BoundUnaryExpression::new(
                origin, operator, operands, None, recovered,
            ))
        } else {
            BoundExpression::Binary(BoundBinaryExpression::new(
                origin, operator, operands, None, recovered,
            ))
        };

        self.push(binder, expression)
    }

    pub(super) fn bind_structured<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
        kind: BoundStructuredExpressionKind,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let loop_target = matches!(
            kind,
            BoundStructuredExpressionKind::While | BoundStructuredExpressionKind::Loop
        )
        .then(|| ControlTarget::new(ControlTargetKind::Loop, SyntaxAnchor::from_node(&syntax)));

        if let Some(target) = loop_target {
            binder.push_control_target(target);
        }

        let captures_yield = !matches!(
            kind,
            BoundStructuredExpressionKind::While | BoundStructuredExpressionKind::Loop
        );

        let children = self.bind_semantic_children(binder, scope, syntax, captures_yield);

        if let Some(target) = loop_target
            && binder.pop_control_target() != Some(target)
        {
            return Err(BindingError::ControlTargetMismatch);
        }

        let (operands, blocks) = children?;

        let recovered = syntax.is_recovered()
            || operands
                .iter()
                .any(|operand| binder.expression_is_recovered(*operand))
            || blocks.iter().any(|block| binder.block_is_recovered(*block));

        let mut expression = BoundStructuredExpression::new(
            binder.source_origin(&syntax),
            kind,
            operands,
            blocks,
            [],
            None,
            recovered,
        );

        if kind == BoundStructuredExpressionKind::Borrow {
            let Some(borrow) = syntax.cast::<BorrowExpressionSyntax>() else {
                return Err(BindingError::UnsupportedSyntax);
            };

            let borrow_kind = if borrow.mut_keyword().is_some() {
                BorrowKind::Mutable
            } else {
                BorrowKind::Shared
            };

            expression = expression.with_borrow_kind(borrow_kind);
        }

        self.push(binder, BoundExpression::Structured(expression))
    }

    pub(in crate::binding::expression) fn bind_structured_with_operand<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
        kind: BoundStructuredExpressionKind,
        operand: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let (children, blocks) = self.bind_semantic_children(binder, scope, syntax, true)?;

        let recovered = syntax.is_recovered()
            || binder.expression_is_recovered(operand)
            || children
                .iter()
                .any(|child| binder.expression_is_recovered(*child))
            || blocks.iter().any(|block| binder.block_is_recovered(*block));

        let operands = std::iter::once(operand).chain(children).collect::<Vec<_>>();

        let mut expression = BoundStructuredExpression::new(
            binder.source_origin(&syntax),
            kind,
            operands.iter().copied(),
            blocks,
            [],
            None,
            recovered,
        );

        if kind == BoundStructuredExpressionKind::SliceIndex {
            let Some(slice) = syntax.cast::<SliceIndexOperationSyntax>() else {
                return Err(BindingError::UnsupportedSyntax);
            };

            expression = expression.with_slice_bounds(slice_bounds(&slice, &operands)?);
        }

        self.push(binder, BoundExpression::Structured(expression))
    }

    fn bind_semantic_children<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
        captures_yield: bool,
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

                match self.bind_expression(binder, scope, Some(&expression)) {
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

                let result = if captures_yield {
                    binder.bind_block(scope, &block, self)
                } else {
                    binder.bind_non_yielding_block(scope, &block, self)
                };

                match result {
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

    pub(super) fn bind_first_descendant_expression<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
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

            result = Some(self.bind_expression(binder, scope, Some(&expression)));

            SyntaxWalkControl::Stop
        });

        match result {
            Some(result) => result,
            None => self.push_error(binder, Some(recovery_origin)),
        }
    }

    pub(in crate::binding::expression) fn path_context_for(
        &self,
        scope: LocalScopeId,
        access: NameAccess,
    ) -> PathBindingContext {
        match self.path_context.module() {
            Some(module) => {
                PathBindingContext::new(scope, module, self.path_context.module_owner(), access)
            }
            None => {
                PathBindingContext::without_module(scope, self.path_context.module_owner(), access)
            }
        }
    }

    pub(in crate::binding::expression) fn push<C>(
        &self,
        binder: &mut Binder<'_, C>,
        expression: BoundExpression,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        binder
            .unit_mut()
            .tree_mut()
            .push_expression(expression)
            .map_err(crate::unit::BoundUnitConstructionError::from)
            .map_err(Into::into)
    }

    pub(super) fn push_error<C>(
        &self,
        binder: &mut Binder<'_, C>,
        syntax: Option<&impl SourceSyntaxNode>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let origin = syntax.map_or_else(
            || bray_bound_tree::BoundNodeOrigin::source(binder.unit().key().source()),
            |syntax| binder.source_origin(syntax),
        );

        self.push(
            binder,
            BoundExpression::Error(BoundErrorExpression::new(origin, self.error_type)),
        )
    }

    fn push_missing_error<C>(&self, binder: &mut Binder<'_, C>) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let origin = bray_bound_tree::BoundNodeOrigin::source(binder.unit().key().source());

        self.push(
            binder,
            BoundExpression::Error(BoundErrorExpression::new(origin, self.error_type)),
        )
    }
}

fn slice_bounds(
    syntax: &SliceIndexOperationSyntax,
    operands: &[BoundExpressionId],
) -> BindingResult<BoundSliceBounds> {
    let Some((_, bounds)) = operands.split_first() else {
        return Err(BindingError::UnsupportedSyntax);
    };

    let dot_dot = syntax.dot_dot_token().range().start();
    let syntax_bounds = syntax.expressions().collect::<Vec<_>>();

    match (syntax_bounds.as_slice(), bounds) {
        ([], []) => Ok(BoundSliceBounds::new(None, None)),
        ([bound], [expression]) if bound.full_range().end() <= dot_dot => {
            Ok(BoundSliceBounds::new(Some(*expression), None))
        }
        ([bound], [expression]) if bound.full_range().start() >= dot_dot => {
            Ok(BoundSliceBounds::new(None, Some(*expression)))
        }
        ([_, _], [lower, upper]) => Ok(BoundSliceBounds::new(Some(*lower), Some(*upper))),
        _ => Err(BindingError::UnsupportedSyntax),
    }
}

impl<C> BlockBindingOperations<C> for ExpressionBinder
where
    C: BinderFactContext + ?Sized,
{
    fn bind_expression(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<BoundExpressionId> {
        ExpressionBinder::bind_expression(self, binder, scope, syntax)
    }

    fn bind_generator_expression(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &bray_syntax::GeneratorIterationExpressionSyntax,
    ) -> BindingResult<BoundExpressionId> {
        self.bind_generator_iteration(binder, scope, syntax)
    }

    fn bind_type_expression(
        &mut self,
        _request: &mut Binder<'_, C>,
        _scope: LocalScopeId,
        _syntax: Option<&TypeExpressionSyntax>,
    ) -> BindingResult<Option<BoundTypeReference>> {
        Ok(_syntax.map(|syntax| BoundTypeReference::new(SyntaxAnchor::from_node(syntax), None)))
    }

    fn error_type(&self) -> TypeId {
        self.error_type
    }

    fn path_context(
        &self,
        _request: &Binder<'_, C>,
        scope: LocalScopeId,
    ) -> BindingResult<PathBindingContext> {
        Ok(self.path_context_for(scope, self.path_context.access()))
    }
}

#[cfg(test)]
mod tests {
    use crate::fact::test_support::TestFixture;
    use bray_bound_tree::{
        BoundControlTransferKind, BoundExpression, BoundLiteralKind, BoundOperator,
        BoundStructuredExpressionKind, BoundWalkControl, BoundWalkEvent, walk_bound_tree,
    };

    #[test]
    fn expression_blocks_bind_names_operators_calls_and_control_flow() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    size + size;\n",
            "    size<i32, 2>(value = size);\n",
            "    size.field;\n",
            "    size as i32;\n",
            "    [size, size];\n",
            "    [size; 2];\n",
            "    size[..size];\n",
            "    size[size..];\n",
            "    if size {};\n",
            "    for item in mut size\n",
            "    {\n",
            "        yield item;\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        yield none;\n",
            "    };\n",
            "    {\n",
            "        each item in size\n",
            "        {\n",
            "            yield item;\n",
            "        }\n",
            "    };\n",
            "    [\n",
            "        each item in size\n",
            "        {\n",
            "            yield item;\n",
            "        }\n",
            "    ];\n",
            "    match size\n",
            "    {\n",
            "        case size\n",
            "        {\n",
            "            yield size;\n",
            "        }\n",
            "    };\n",
            "    loop\n",
            "    {\n",
            "        break;\n",
            "    };\n",
            "    lambda(value: i32) {};\n",
            "    await size;\n",
            "    &size;\n",
            "    & mut size;\n",
            "    with resource: i32 = size\n",
            "    {\n",
            "        resource;\n",
            "    };\n",
            "    return size;\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, syntax) = crate::binding::test_support::binder_and_block(&facts);

        let root_scope = binder.unit().root_scope();
        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

        let block = match binder.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("valid expression block must bind: {error:?}"),
        };

        let result = match binder.finish() {
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
        let mut saw_array = false;
        let mut saw_repeated_array = false;
        let mut saw_omitted_lower_slice_bound = false;
        let mut saw_omitted_upper_slice_bound = false;
        let mut saw_integer_literal = false;
        let mut saw_conditional = false;
        let mut saw_for_pattern = false;
        let mut saw_general_generator = false;
        let mut saw_array_generator = false;
        let mut saw_match_pattern = false;
        let mut saw_targeted_break = false;
        let mut saw_targeted_return = false;
        let mut saw_lambda = false;
        let mut saw_await = false;
        let mut saw_shared_borrow = false;
        let mut saw_mutable_borrow = false;
        let mut saw_with_pattern = false;

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

            assert!(expression.ty().is_none());

            match expression {
                BoundExpression::Binary(expression)
                    if expression.operator() == BoundOperator::Add =>
                {
                    saw_add = true;
                }
                BoundExpression::Call(expression)
                    if expression.arguments().len() == 1
                        && expression.generic_arguments().len() == 2
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
                BoundExpression::MemberAccess(expression)
                    if matches!(
                        expression.selector(),
                        Some(bray_bound_tree::BoundMemberSelector::Name(name))
                            if name.as_str() == "field"
                    ) =>
                {
                    saw_named_member = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Array =>
                {
                    saw_array = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::RepeatedArray =>
                {
                    saw_repeated_array = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::SliceIndex =>
                {
                    let Some(bounds) = expression.slice_bounds() else {
                        panic!("slice expression must preserve its exact bound shape");
                    };

                    saw_omitted_lower_slice_bound |=
                        bounds.lower().is_none() && bounds.upper().is_some();

                    saw_omitted_upper_slice_bound |=
                        bounds.lower().is_some() && bounds.upper().is_none();
                }
                BoundExpression::Literal(expression)
                    if expression.kind() == BoundLiteralKind::Integer =>
                {
                    saw_integer_literal = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Conditional =>
                {
                    saw_conditional = true;
                }
                BoundExpression::For(_) => {
                    saw_for_pattern = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::GeneralGenerator =>
                {
                    saw_general_generator = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::ArrayGenerator =>
                {
                    saw_array_generator = true;
                }
                BoundExpression::Match(expression) if !expression.arms().is_empty() => {
                    saw_match_pattern = true;
                }
                BoundExpression::ControlTransfer(expression)
                    if expression.kind() == BoundControlTransferKind::Break
                        && expression.target().is_some() =>
                {
                    saw_targeted_break = true;
                }
                BoundExpression::ControlTransfer(expression)
                    if expression.kind() == BoundControlTransferKind::Return
                        && expression.target().is_some() =>
                {
                    saw_targeted_return = true;
                }
                BoundExpression::AnonymousCallable(_) => saw_lambda = true,
                BoundExpression::Await(expression) => {
                    assert_eq!(
                        expression.resolution(),
                        bray_bound_tree::BoundAwaitResolution::Pending
                    );

                    saw_await = true;
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Borrow =>
                {
                    match expression.borrow_kind() {
                        Some(bray_symbols::BorrowKind::Shared) => saw_shared_borrow = true,
                        Some(bray_symbols::BorrowKind::Mutable) => saw_mutable_borrow = true,
                        None => panic!("bound borrow expressions must retain their borrow kind"),
                    }
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::With
                        && expression.patterns().len() == 1 =>
                {
                    saw_with_pattern = true;
                }
                _ => {}
            }

            BoundWalkControl::Continue
        });

        assert!(saw_add);
        assert!(saw_call);
        assert!(saw_conversion);
        assert!(saw_named_member);
        assert!(saw_array);
        assert!(saw_repeated_array);
        assert!(saw_omitted_lower_slice_bound);
        assert!(saw_omitted_upper_slice_bound);
        assert!(saw_integer_literal);
        assert!(saw_conditional);
        assert!(saw_for_pattern);
        assert!(saw_general_generator);
        assert!(saw_array_generator);
        assert!(saw_match_pattern);
        assert!(saw_targeted_break);
        assert!(saw_targeted_return);
        assert!(saw_lambda);
        assert!(saw_await);
        assert!(saw_shared_borrow);
        assert!(saw_mutable_borrow);
        assert!(saw_with_pattern);

        assert_eq!(result.dependencies().len(), 1);
    }

    #[test]
    fn unresolved_expressions_recover_without_losing_the_enclosing_block() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    missing;\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, syntax) = crate::binding::test_support::binder_and_block(&facts);

        let root_scope = binder.unit().root_scope();
        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

        let block = match binder.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("malformed expression block must recover: {error:?}"),
        };

        let result = match binder.finish() {
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

        let Some(BoundExpression::UnresolvedReference(expression)) =
            result.unit().tree().expression(*expression)
        else {
            panic!("unresolved name must retain reference recovery");
        };

        assert_eq!(
            expression.kind(),
            bray_bound_tree::BoundUnresolvedReferenceKind::NotFound
        );

        assert!(expression.candidates().is_empty());
    }

    #[test]
    fn malformed_calls_and_conversions_preserve_category_specific_recovery() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    missing<i32 2>(value = );\n",
            "    missing as ;\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, syntax) = crate::binding::test_support::binder_and_block(&facts);

        let root_scope = binder.unit().root_scope();
        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

        let block = match binder.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("malformed expressions must recover: {error:?}"),
        };

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("recovered expressions must freeze: {error:?}"),
        };

        let Some(block) = result.unit().tree().block(block) else {
            panic!("recovered block must remain in the tree");
        };

        let [
            bray_bound_tree::BoundBlockItem::Expression(call),
            bray_bound_tree::BoundBlockItem::Expression(conversion),
        ] = block.items()
        else {
            panic!("recovered block must retain both expressions");
        };

        let Some(BoundExpression::ErrorCall(call)) = result.unit().tree().expression(*call) else {
            panic!("malformed call must retain call recovery");
        };

        assert_eq!(call.generic_arguments().len(), 2);
        assert_eq!(call.arguments().len(), 1);

        assert_eq!(
            call.arguments()[0]
                .name()
                .map(bray_symbols::SymbolName::as_str),
            Some("value")
        );

        let Some(BoundExpression::ErrorConversion(conversion)) =
            result.unit().tree().expression(*conversion)
        else {
            panic!("malformed conversion must retain conversion recovery");
        };

        assert!(conversion.target_syntax().is_recovered());
        assert_eq!(result.diagnostics().len(), 2);
    }

    #[test]
    fn expression_binding_preserves_category_specific_source_relationships() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    origin: i32;\n",
            "}\n",
            "trait Reader\n",
            "{\n",
            "}\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    Point.origin;\n",
            "    Point\n",
            "    {\n",
            "        origin = size,\n",
            "    };\n",
            "    {\n",
            "        origin = size,\n",
            "    };\n",
            "    size(Reader).read;\n",
            "    {\n",
            "        each item in size\n",
            "        {\n",
            "            yield item;\n",
            "        }\n",
            "    };\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, syntax) = crate::binding::test_support::binder_and_block(&facts);

        let root_scope = binder.unit().root_scope();
        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

        let block = match binder.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("binding must succeed: {error:?}"),
        };

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("binding result must publish: {error:?}"),
        };

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let mut explicit_construction = false;
        let mut expected_construction = false;
        let mut trait_qualified = false;
        let mut generator_iteration = None;
        let mut generator_region = None;
        let mut yield_target = None;

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
                BoundExpression::StructConstruction(expression)
                    if expression.fields().len() == 1
                        && expression.fields()[0]
                            .name()
                            .is_some_and(|name| name.as_str() == "origin") =>
                {
                    if expression.head().is_some() {
                        explicit_construction = true;
                    } else {
                        expected_construction = true;
                    }
                }
                BoundExpression::TraitQualifiedMember(expression)
                    if expression
                        .selector()
                        .is_some_and(|selector| matches!(selector, bray_bound_tree::BoundMemberSelector::Name(name) if name.as_str() == "read")) =>
                {
                    trait_qualified = true;
                }
                BoundExpression::Generator(expression) => {
                    generator_iteration = Some(expression.region());
                }
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::GeneralGenerator =>
                {
                    generator_region = Some(expression.origin().source_anchor().syntax());
                }
                BoundExpression::ControlTransfer(expression)
                    if expression.kind() == BoundControlTransferKind::Yield =>
                {
                    yield_target = expression.target();
                }
                _ => {}
            }

            BoundWalkControl::Continue
        });

        assert!(explicit_construction);
        assert!(expected_construction);
        assert!(trait_qualified);
        assert_eq!(yield_target, generator_region);
        assert_ne!(yield_target, generator_iteration);
    }
}
