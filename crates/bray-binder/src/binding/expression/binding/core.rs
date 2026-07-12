use bray_bound_tree::{
    BoundAssignmentExpression, BoundBinaryExpression, BoundErrorExpression, BoundExpression,
    BoundExpressionId, BoundOperator, BoundStructuredExpression, BoundStructuredExpressionKind,
    BoundTypeReference, BoundUnaryExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeId, TypeId};
use bray_syntax::{
    ExpressionSyntax, PrimaryExpressionSyntax, SourceSyntaxNode, SyntaxKind, SyntaxNodeView,
    SyntaxWalkControl, SyntaxWalkEvent, TypeExpressionSyntax, walk_syntax_node,
};

use super::super::super::block::BlockBindingOperations;
use super::super::super::{BindingError, BindingResult};
use super::super::support::classify_operator;
use crate::BinderFactContext;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::request::{BinderRequestContext, ControlTarget, ControlTargetKind};

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

        let expression = if let Some(primary) = syntax.primary_expression() {
            self.bind_primary(request, scope, &primary)?
        } else if let Some(operator) = syntax.operator_token() {
            self.bind_operator(request, scope, syntax, operator.kind())?
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

        self.push(request, expression)
    }

    pub(super) fn bind_structured<C>(
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
            BoundStructuredExpressionKind::While | BoundStructuredExpressionKind::Loop
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
            None,
            recovered,
        );

        self.push(request, BoundExpression::Structured(expression))
    }

    pub(in crate::binding::expression) fn bind_structured_with_operand<C>(
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
            None,
            recovered,
        );

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

    pub(super) fn bind_first_descendant_expression<C>(
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

    pub(in crate::binding::expression) fn path_context_for(
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

    pub(in crate::binding::expression) fn push<C>(
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

    pub(super) fn push_error<C>(
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
        self.bind_generator_iteration(request, scope, syntax, SyntaxAnchor::from_node(syntax))
    }

    fn bind_type_expression(
        &mut self,
        _request: &mut BinderRequestContext<'_, C>,
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
        _request: &BinderRequestContext<'_, C>,
        scope: LocalScopeId,
    ) -> BindingResult<PathBindingContext> {
        Ok(self.path_context_for(scope, self.path_context.access()))
    }
}

#[cfg(test)]
mod tests {
    use crate::fact::test_support::TestFixture;
    use bray_bound_tree::{
        BoundControlTransferKind, BoundExpression, BoundOperator, BoundSpawnInput, BoundSpawnMode,
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
            "    size(value = size);\n",
            "    size.field;\n",
            "    size as i32;\n",
            "    if size\n",
            "    {\n",
            "    };\n",
            "    for item in mut size\n",
            "    {\n",
            "        yield item;\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        yield none;\n",
            "    };\n",
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
            "    lambda(value: i32)\n",
            "    {\n",
            "    };\n",
            "    return size;\n",
            "}",
        ));

        let facts = fixture.context();
        let (mut request, syntax) = crate::binding::test_support::request_and_block(&facts);
        let root_scope = request.unit().root_scope();

        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

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

            assert!(expression.ty().is_none());

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
                    if expression.kind() == BoundStructuredExpressionKind::Conditional =>
                {
                    saw_conditional = true;
                }
                BoundExpression::For(_) => {
                    saw_for_pattern = true;
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
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    Missing;\n",
            "}",
        ));

        let facts = fixture.context();
        let (mut request, syntax) = crate::binding::test_support::request_and_block(&facts);
        let root_scope = request.unit().root_scope();

        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

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
            "    spawn size;\n",
            "    spawn detached size;\n",
            "    spawn thread size(value = size);\n",
            "}",
        ));

        let facts = fixture.context();
        let (mut request, syntax) = crate::binding::test_support::request_and_block(&facts);
        let root_scope = request.unit().root_scope();

        let path_context = crate::binding::test_support::internal_path_context(&facts, root_scope);

        let block = match request.bind_callable_body_block(
            root_scope,
            &syntax,
            path_context,
            fixture.declared_type,
        ) {
            Ok(block) => block,
            Err(error) => panic!("binding must succeed: {error:?}"),
        };

        let result = match request.finish() {
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
        let mut generator_region = None;
        let mut yield_target = None;
        let mut spawn_modes = Vec::new();
        let mut thread_arguments = None;

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
                    generator_region = Some(expression.region());
                }
                BoundExpression::ControlTransfer(expression)
                    if expression.kind() == BoundControlTransferKind::Yield =>
                {
                    yield_target = expression.target();
                }
                BoundExpression::Spawn(expression) => {
                    spawn_modes.push(expression.mode());

                    if let BoundSpawnInput::Thread { arguments, .. } = expression.input() {
                        thread_arguments = Some(arguments.len());
                    }
                }
                _ => {}
            }

            BoundWalkControl::Continue
        });

        assert!(explicit_construction);
        assert!(expected_construction);
        assert!(trait_qualified);
        assert_eq!(yield_target, generator_region);
        assert_eq!(
            spawn_modes,
            [
                BoundSpawnMode::Task,
                BoundSpawnMode::DetachedTask,
                BoundSpawnMode::Thread,
            ]
        );
        assert_eq!(thread_arguments, Some(1));
    }
}
