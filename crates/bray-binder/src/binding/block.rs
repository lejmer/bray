use bray_bound_tree::{
    BoundBlock, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundLocalBinding,
    BoundLocalConstant, BoundReferenceTarget, BoundTypeReference,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId, SymbolOrdinal, TypeId};
use bray_syntax::{
    BlockExpressionSyntax, ConstantDeclarationSyntax, ExpressionSyntax,
    GeneratorIterationExpressionSyntax, LocalBindingDeclarationSyntax, SourceSyntaxNode,
    TypeExpressionSyntax,
};

use super::BindingResult;
use super::name::{name_is_available, symbol_name};
use crate::BindingQueryContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind, PatternBindingMode};
use crate::lookup::PathBindingContext;

pub(crate) trait BlockBindingOperations<C>
where
    C: BindingQueryContext + ?Sized,
{
    fn bind_expression(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>;

    fn bind_generator_expression(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &GeneratorIterationExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>;

    fn bind_type_expression(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&TypeExpressionSyntax>,
    ) -> BindingResult<Option<BoundTypeReference>, C::UpstreamError>;

    fn error_type(&self) -> TypeId;

    fn path_context(
        &self,
        binder: &Binder<'_, C>,
        scope: LocalScopeId,
    ) -> BindingResult<PathBindingContext, C::UpstreamError>;
}

impl<C> Binder<'_, C>
where
    C: BindingQueryContext + ?Sized,
{
    pub(crate) fn bind_block(
        &mut self,
        parent_scope: LocalScopeId,
        syntax: &BlockExpressionSyntax,
        operations: &mut impl BlockBindingOperations<C>,
    ) -> BindingResult<BoundBlockId, C::UpstreamError> {
        self.bind_transaction(|binder| {
            binder.bind_block_transaction(parent_scope, syntax, operations, true)
        })
    }

    pub(crate) fn bind_non_yielding_block(
        &mut self,
        parent_scope: LocalScopeId,
        syntax: &BlockExpressionSyntax,
        operations: &mut impl BlockBindingOperations<C>,
    ) -> BindingResult<BoundBlockId, C::UpstreamError> {
        self.bind_transaction(|binder| {
            binder.bind_block_transaction(parent_scope, syntax, operations, false)
        })
    }

    fn bind_block_transaction(
        &mut self,
        parent_scope: LocalScopeId,
        syntax: &BlockExpressionSyntax,
        operations: &mut impl BlockBindingOperations<C>,
        captures_yield: bool,
    ) -> BindingResult<BoundBlockId, C::UpstreamError> {
        self.check_cancellation()?;

        let scope = self.unit_mut().push_scope(
            parent_scope,
            LocalScopeBoundary::Block,
            SyntaxAnchor::from_node(syntax),
            syntax.open_brace_token().range().end(),
        )?;

        let target = captures_yield
            .then(|| ControlTarget::new(ControlTargetKind::Block, SyntaxAnchor::from_node(syntax)));

        if let Some(target) = target {
            self.push_control_target(target);
        }

        let mut items = Vec::new();

        for item in syntax.block_items() {
            self.check_cancellation()?;

            if let Some(declaration) = item.local_binding_declaration() {
                items.push(BoundBlockItem::LocalBinding(self.bind_local_binding(
                    scope,
                    &declaration,
                    operations,
                )?));
            } else if let Some(declaration) = item.constant_declaration() {
                items.push(BoundBlockItem::LocalConstant(self.bind_local_constant(
                    scope,
                    &declaration,
                    operations,
                )?));
            } else if let Some(expression) = item.expression() {
                let expression = operations.bind_expression(self, scope, Some(&expression))?;

                items.push(BoundBlockItem::Expression(expression));
            } else if let Some(expression) = item.generator_iteration_expression() {
                let expression = operations.bind_generator_expression(self, scope, &expression)?;

                items.push(BoundBlockItem::Expression(expression));
            }
        }

        if let Some(target) = target
            && self.pop_control_target() != Some(target)
        {
            return Err(super::BindingError::ControlTargetMismatch);
        }

        let is_recovered = syntax.is_recovered()
            || items
                .iter()
                .any(|item| bound_block_item_is_recovered(self, item));

        let block = BoundBlock::new(self.source_origin(syntax), items, is_recovered);

        self.unit_mut()
            .tree_mut()
            .push_block(block)
            .map_err(crate::unit::BoundUnitConstructionError::from)
            .map_err(Into::into)
    }

    fn bind_local_binding(
        &mut self,
        scope: LocalScopeId,
        syntax: &LocalBindingDeclarationSyntax,
        operations: &mut impl BlockBindingOperations<C>,
    ) -> BindingResult<BoundLocalBinding, C::UpstreamError> {
        let declared_type = match syntax.type_annotation() {
            Some(annotation) => {
                operations.bind_type_expression(self, scope, Some(&annotation.type_expression()))?
            }
            None => None,
        };

        let initializer = operations.bind_expression(self, scope, syntax.expression().as_ref())?;

        let input_type = declared_type
            .and_then(BoundTypeReference::ty)
            .unwrap_or_else(|| {
                self.unit_view()
                    .expression(initializer)
                    .and_then(bray_bound_tree::BoundExpression::ty)
                    .unwrap_or_else(|| operations.error_type())
            });

        let context = operations.path_context(self, scope)?;

        let pattern = self.bind_irrefutable_pattern(
            context,
            &syntax.irrefutable_pattern(),
            input_type,
            operations.error_type(),
            PatternBindingMode::Declaration,
        )?;

        self.activate_pattern_bindings(scope, &pattern)?;

        Ok(BoundLocalBinding::new(
            self.source_origin(syntax),
            pattern.pattern(),
            pattern.bindings().iter().copied(),
            declared_type,
            initializer,
            syntax.is_recovered(),
        ))
    }

    fn bind_local_constant(
        &mut self,
        scope: LocalScopeId,
        syntax: &ConstantDeclarationSyntax,
        operations: &mut impl BlockBindingOperations<C>,
    ) -> BindingResult<BoundLocalConstant, C::UpstreamError> {
        let declared_type = operations
            .bind_type_expression(self, scope, Some(&syntax.type_expression()))?
            .unwrap_or_else(|| {
                BoundTypeReference::new(SyntaxAnchor::from_node(&syntax.type_expression()), None)
            });

        let initializer = operations.bind_expression(self, scope, syntax.expression().as_ref())?;

        let anchor = SyntaxAnchor::from_node(syntax);
        let token = syntax.identifier_token();
        let context = operations.path_context(self, scope)?;

        let symbol = match symbol_name(syntax.source(), &token) {
            Some(name) if name_is_available(self, context, syntax.source(), &token) => {
                let symbol = self.unit_mut().push_constant(
                    scope,
                    name,
                    [anchor],
                    Some(SymbolOrdinal::new(0)),
                    syntax.is_recovered(),
                )?;

                self.unit_mut().activate_local(scope, symbol)?;

                if let Some(ty) = declared_type.ty() {
                    self.record_value_type(BoundReferenceTarget::Local(symbol.into()), ty);
                }

                Some(symbol)
            }
            Some(_) | None => None,
        };

        Ok(BoundLocalConstant::new(
            self.source_origin(syntax),
            symbol,
            declared_type,
            initializer,
            syntax.is_recovered(),
        ))
    }
}

fn bound_block_item_is_recovered<C>(binder: &Binder<'_, C>, item: &BoundBlockItem) -> bool
where
    C: BindingQueryContext + ?Sized,
{
    match item {
        BoundBlockItem::LocalBinding(binding) => {
            binding.is_recovered()
                || binder.expression_is_recovered(binding.initializer())
                || binder.pattern_is_recovered(binding.pattern())
        }
        BoundBlockItem::LocalConstant(constant) => {
            constant.is_recovered() || binder.expression_is_recovered(constant.initializer())
        }
        BoundBlockItem::Expression(expression) => binder.expression_is_recovered(*expression),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBlockItem, BoundErrorExpression, BoundExpression, BoundNodeOrigin, BoundTypeReference,
    };
    use bray_declarations::SyntaxAnchor;
    use bray_symbols::{LocalScopeBoundary, TypeId};
    use bray_syntax::{ExpressionSyntax, GeneratorIterationExpressionSyntax, TypeExpressionSyntax};

    use super::BlockBindingOperations;
    use crate::BindingQueryContext;
    use crate::binder::Binder;
    use crate::binder::ControlTargetKind;
    use crate::binding::{BindingError, BindingResult};
    use crate::query::test_support::TestFixture;

    #[test]
    fn blocks_bind_local_items_in_source_order_and_activate_after_initializers() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = 1;\n",
            "    const local: i32 = 2;\n",
            "    value;\n",
            "}",
        ));

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let root = binder.unit().root_scope();
        let mut operations = TestOperations::new(fixture.declared_type);

        let block_id = match binder.bind_block(root, &block, &mut operations) {
            Ok(block) => block,
            Err(error) => panic!("valid block must bind: {error:?}"),
        };

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("bound block must freeze: {error:?}"),
        };

        let Some(block) = result.unit().tree().block(block_id) else {
            panic!("bound block must be published");
        };

        assert!(matches!(block.items()[0], BoundBlockItem::LocalBinding(_)));
        assert!(matches!(block.items()[1], BoundBlockItem::LocalConstant(_)));
        assert!(matches!(block.items()[2], BoundBlockItem::Expression(_)));

        assert_eq!(operations.visible_names, vec![(0, 0), (1, 0), (1, 1)]);

        assert_eq!(
            operations.control_targets,
            vec![ControlTargetKind::Block; 3]
        );

        let Some(scope) = result
            .unit()
            .local_symbols()
            .scopes()
            .iter()
            .find(|scope| scope.boundary() == LocalScopeBoundary::Block)
        else {
            panic!("bound block must publish its lexical scope");
        };

        assert_eq!(scope.local_symbols_named("value").len(), 1);
        assert_eq!(scope.local_symbols_named("local").len(), 1);

        let BoundBlockItem::LocalBinding(binding) = &block.items()[0] else {
            panic!("first item must remain a local binding");
        };

        let Some(declared_type) = binding.declared_type() else {
            panic!("typed local binding must retain its type reference");
        };

        assert_eq!(
            declared_type.syntax().syntax_kind(),
            bray_syntax::SyntaxKind::TypeExpression
        );

        let BoundBlockItem::LocalConstant(constant) = &block.items()[1] else {
            panic!("second item must remain a local constant");
        };

        assert_eq!(
            constant.declared_type().syntax().syntax_kind(),
            bray_syntax::SyntaxKind::TypeExpression
        );

        let Some(pattern) = result.unit().tree().pattern(binding.pattern()) else {
            panic!("local binding must retain its pattern");
        };

        assert_eq!(pattern.bindings().len(), 1);

        assert_eq!(
            pattern.mode(),
            bray_bound_tree::BoundPatternMode::Declaration
        );
    }

    #[test]
    fn failed_block_binding_rolls_back_nodes_scopes_and_local_identities() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    let value = 1;\n",
            "}",
        ));

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let root = binder.unit().root_scope();
        let mut operations = TestOperations::failing(fixture.declared_type);
        let result = binder.bind_block(root, &block, &mut operations);

        assert_eq!(result, Err(BindingError::Cancelled));

        let created = operations.created_expressions.clone();

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("rolled-back binder must freeze: {error:?}"),
        };

        for expression in created {
            assert!(result.unit().tree().expression(expression).is_none());
        }

        assert!(result.unit().local_symbols().bindings().is_empty());
        assert_eq!(result.unit().local_symbols().scopes().len(), 1);
    }

    #[test]
    fn malformed_local_declarations_publish_recovery_without_empty_names() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    const : i32 = 1;\n",
            "    let = 2;\n",
            "}",
        ));

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let root = binder.unit().root_scope();
        let mut operations = TestOperations::new(fixture.declared_type);

        let block = match binder.bind_block(root, &block, &mut operations) {
            Ok(block) => block,
            Err(error) => panic!("malformed locals must recover: {error:?}"),
        };

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("recovered locals must freeze: {error:?}"),
        };

        let Some(block) = result.unit().tree().block(block) else {
            panic!("recovered block must be published");
        };

        let BoundBlockItem::LocalConstant(constant) = &block.items()[0] else {
            panic!("first recovered item must remain a local constant");
        };

        assert_eq!(constant.symbol(), None);

        let BoundBlockItem::LocalBinding(binding) = &block.items()[1] else {
            panic!("second recovered item must remain a local binding");
        };

        let Some(pattern) = result.unit().tree().pattern(binding.pattern()) else {
            panic!("recovered local binding must retain a pattern");
        };

        assert!(pattern.bindings().is_empty());
        assert!(pattern.is_recovered());
        assert!(result.unit().local_symbols().bindings().is_empty());
        assert!(result.unit().local_symbols().constants().is_empty());
    }

    #[test]
    fn local_declarations_reject_shadowing_and_retain_destructured_identities() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    let (left, right) = 1;\n",
            "    let left = 2;\n",
            "    const size: i32 = 3;\n",
            "}",
        ));

        let binding_context = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&binding_context);

        let root = binder.unit().root_scope();
        let mut operations = TestOperations::new(fixture.declared_type);

        let block = match binder.bind_block(root, &block, &mut operations) {
            Ok(block) => block,
            Err(error) => panic!("shadowing declarations must recover: {error:?}"),
        };

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("recovered block must freeze: {error:?}"),
        };

        bray_testing::assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            bray_diagnostics::DiagnosticKind::BindingNameAlreadyDefined,
        );

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [
                bray_diagnostics::DiagnosticKind::BindingNameAlreadyDefined,
                bray_diagnostics::DiagnosticKind::BindingNameAlreadyDefined,
            ]
        );

        for diagnostic in result.diagnostics() {
            let [prior] = diagnostic.related_locations() else {
                panic!("shadowing diagnostic must retain one prior definition: {diagnostic:?}");
            };

            assert_eq!(
                prior.kind(),
                bray_diagnostics::DiagnosticRelatedLocationKind::FirstDeclaration
            );

            assert!(Some(prior.span()) != diagnostic.primary_span());
        }

        let Some(block) = result.unit().tree().block(block) else {
            panic!("bound block must be published");
        };

        let BoundBlockItem::LocalBinding(binding) = &block.items()[0] else {
            panic!("first item must remain a destructuring binding");
        };

        assert_eq!(binding.bindings().len(), 2);
        assert_eq!(result.unit().local_symbols().bindings().len(), 2);
        assert!(result.unit().local_symbols().constants().is_empty());
    }

    struct TestOperations {
        error_type: TypeId,
        visible_names: Vec<(usize, usize)>,
        control_targets: Vec<ControlTargetKind>,
        created_expressions: Vec<bray_bound_tree::BoundExpressionId>,
        fail_after_expression: bool,
    }

    impl TestOperations {
        fn new(error_type: TypeId) -> Self {
            Self {
                error_type,
                visible_names: Vec::new(),
                control_targets: Vec::new(),
                created_expressions: Vec::new(),
                fail_after_expression: false,
            }
        }

        fn failing(error_type: TypeId) -> Self {
            Self {
                fail_after_expression: true,
                ..Self::new(error_type)
            }
        }
    }

    impl<C> BlockBindingOperations<C> for TestOperations
    where
        C: BindingQueryContext + ?Sized,
    {
        fn bind_expression(
            &mut self,
            binder: &mut Binder<'_, C>,
            scope: bray_symbols::LocalScopeId,
            syntax: Option<&ExpressionSyntax>,
        ) -> BindingResult<bray_bound_tree::BoundExpressionId, C::UpstreamError> {
            let values = binder.unit().local_symbols_named(scope, "value")?.len();
            let constants = binder.unit().local_symbols_named(scope, "local")?.len();

            self.visible_names.push((values, constants));

            let Some(target) = binder.control_target() else {
                panic!("block expressions must bind under a control target");
            };

            self.control_targets.push(target.kind());

            let origin = syntax.map_or_else(
                || BoundNodeOrigin::source(binder.unit().key().source()),
                |syntax| binder.source_origin(syntax),
            );

            let expression =
                BoundExpression::Error(BoundErrorExpression::new(origin, self.error_type));

            let expression = binder
                .unit_mut()
                .tree_mut()
                .push_expression(expression)
                .map_err(crate::unit::BoundUnitConstructionError::from)?;

            self.created_expressions.push(expression);

            if self.fail_after_expression {
                return Err(BindingError::Cancelled);
            }

            Ok(expression)
        }

        fn bind_generator_expression(
            &mut self,
            binder: &mut Binder<'_, C>,
            scope: bray_symbols::LocalScopeId,
            _syntax: &GeneratorIterationExpressionSyntax,
        ) -> BindingResult<bray_bound_tree::BoundExpressionId, C::UpstreamError> {
            self.bind_expression(binder, scope, None)
        }

        fn bind_type_expression(
            &mut self,
            _request: &mut Binder<'_, C>,
            _scope: bray_symbols::LocalScopeId,
            syntax: Option<&TypeExpressionSyntax>,
        ) -> BindingResult<Option<BoundTypeReference>, C::UpstreamError> {
            Ok(syntax.map(|syntax| {
                BoundTypeReference::new(SyntaxAnchor::from_node(syntax), Some(self.error_type))
            }))
        }

        fn error_type(&self) -> TypeId {
            self.error_type
        }

        fn path_context(
            &self,
            binder: &Binder<'_, C>,
            scope: bray_symbols::LocalScopeId,
        ) -> BindingResult<crate::lookup::PathBindingContext, C::UpstreamError> {
            Ok(crate::binding::test_support::internal_path_context(
                binder.binding_context(),
                scope,
            ))
        }
    }
}
