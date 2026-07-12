use bray_bound_tree::{
    BoundBlock, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundLocalBinding,
    BoundLocalConstant,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId, SymbolOrdinal, TypeId};
use bray_syntax::{
    BlockExpressionSyntax, ConstantDeclarationSyntax, ExpressionSyntax,
    GeneratorIterationExpressionSyntax, LocalBindingDeclarationSyntax, SourceSyntaxNode,
    TypeExpressionSyntax,
};

use super::BindingResult;
use super::name::symbol_name;
use crate::BinderFactContext;
use crate::request::{
    AbandonedDependencyDisposition, BinderRequestContext, ControlTarget, ControlTargetKind,
    PatternBindingMode,
};

pub(crate) trait BlockBindingOperations<C>
where
    C: BinderFactContext + ?Sized,
{
    fn bind_expression(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<BoundExpressionId>;

    fn bind_generator_expression(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &GeneratorIterationExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>;

    fn bind_type_expression(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&TypeExpressionSyntax>,
    ) -> BindingResult<Option<TypeId>>;

    fn error_type(&self) -> TypeId;
}

impl<C> BinderRequestContext<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn bind_block(
        &mut self,
        parent_scope: LocalScopeId,
        syntax: &BlockExpressionSyntax,
        operations: &mut impl BlockBindingOperations<C>,
    ) -> BindingResult<BoundBlockId> {
        let checkpoint = self.checkpoint();
        let result = self.bind_block_transaction(parent_scope, syntax, operations);

        if result.is_err()
            && !self.rollback(
                checkpoint,
                AbandonedDependencyDisposition::DiscardProvenIrrelevant,
            )
        {
            return Err(super::BindingError::RollbackFailed);
        }

        result
    }

    fn bind_block_transaction(
        &mut self,
        parent_scope: LocalScopeId,
        syntax: &BlockExpressionSyntax,
        operations: &mut impl BlockBindingOperations<C>,
    ) -> BindingResult<BoundBlockId> {
        self.check_cancellation()?;

        let scope = self.unit_mut().push_scope(
            parent_scope,
            LocalScopeBoundary::Block,
            SyntaxAnchor::from_node(syntax),
            syntax.open_brace_token().range().end(),
        )?;

        let target = ControlTarget::new(
            ControlTargetKind::Block,
            SyntaxAnchor::from_node(syntax),
            None,
        );

        self.push_control_target(target);

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
            } else if let Some(expression) = item.sequenced_expression() {
                let expression =
                    operations.bind_expression(self, scope, expression.expression().as_ref())?;

                items.push(BoundBlockItem::Expression(expression));
            } else if let Some(expression) = item.generator_iteration_expression() {
                let expression = operations.bind_generator_expression(self, scope, &expression)?;

                items.push(BoundBlockItem::Expression(expression));
            }
        }

        if self.pop_control_target() != Some(target) {
            return Err(super::BindingError::ControlTargetMismatch);
        }

        let block = BoundBlock::new(self.source_origin(syntax), items, syntax.is_recovered());

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
    ) -> BindingResult<BoundLocalBinding> {
        let declared_type = match syntax.type_annotation() {
            Some(annotation) => {
                operations.bind_type_expression(self, scope, Some(&annotation.type_expression()))?
            }
            None => None,
        };

        let initializer = operations.bind_expression(self, scope, syntax.expression().as_ref())?;

        let input_type = declared_type.unwrap_or_else(|| {
            self.unit_view()
                .expression(initializer)
                .map_or(operations.error_type(), |expression| expression.ty())
        });

        let pattern = self.bind_irrefutable_pattern(
            scope,
            &syntax.irrefutable_pattern(),
            input_type,
            PatternBindingMode::Declaration,
        )?;

        self.activate_pattern_bindings(scope, &pattern)?;

        Ok(BoundLocalBinding::new(
            self.source_origin(syntax),
            pattern.pattern(),
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
    ) -> BindingResult<BoundLocalConstant> {
        let declared_type = operations
            .bind_type_expression(self, scope, Some(&syntax.type_expression()))?
            .unwrap_or_else(|| operations.error_type());

        let initializer = operations.bind_expression(self, scope, syntax.expression().as_ref())?;

        let anchor = SyntaxAnchor::from_node(syntax);
        let token = syntax.identifier_token();

        let symbol = match symbol_name(syntax.source(), &token) {
            Some(name) => {
                let symbol = self.unit_mut().push_constant(
                    scope,
                    name,
                    [anchor],
                    Some(SymbolOrdinal::new(0)),
                    syntax.is_recovered(),
                )?;

                self.unit_mut().activate_local(scope, symbol)?;

                Some(symbol)
            }
            None => None,
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

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundBlockItem, BoundErrorExpression, BoundExpression, BoundNodeOrigin};
    use bray_symbols::{LocalScopeBoundary, TypeId};
    use bray_syntax::{ExpressionSyntax, GeneratorIterationExpressionSyntax, TypeExpressionSyntax};

    use super::BlockBindingOperations;
    use crate::BinderFactContext;
    use crate::binding::{BindingError, BindingResult};
    use crate::fact::test_support::TestFixture;
    use crate::request::BinderRequestContext;
    use crate::request::ControlTargetKind;

    #[test]
    fn blocks_bind_local_items_in_source_order_and_activate_after_initializers() {
        let fixture = TestFixture::from_source(
            "module app; const Size: Int = 1; func main() { let value = 1; const Local: Int = 2; value; }",
        );
        let facts = fixture.context();
        let (mut request, block) = crate::binding::test_support::request_and_block(&facts);
        let root = request.unit().root_scope();
        let mut operations = TestOperations::new(fixture.declared_type);

        let block_id = match request.bind_block(root, &block, &mut operations) {
            Ok(block) => block,
            Err(error) => panic!("valid block must bind: {error:?}"),
        };

        let result = match request.finish() {
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
        assert_eq!(scope.local_symbols_named("Local").len(), 1);

        let BoundBlockItem::LocalBinding(binding) = &block.items()[0] else {
            panic!("first item must remain a local binding");
        };

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
        let fixture = TestFixture::from_source(
            "module app; const Size: Int = 1; func main() { let value = 1; }",
        );
        let facts = fixture.context();
        let (mut request, block) = crate::binding::test_support::request_and_block(&facts);
        let root = request.unit().root_scope();
        let mut operations = TestOperations::failing(fixture.declared_type);

        let result = request.bind_block(root, &block, &mut operations);

        assert_eq!(result, Err(BindingError::Cancelled));

        let created = operations.created_expressions.clone();

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("rolled-back request must freeze: {error:?}"),
        };

        for expression in created {
            assert!(result.unit().tree().expression(expression).is_none());
        }

        assert!(result.unit().local_symbols().bindings().is_empty());
        assert_eq!(result.unit().local_symbols().scopes().len(), 1);
    }

    #[test]
    fn malformed_local_declarations_publish_recovery_without_empty_names() {
        let fixture = TestFixture::from_source(
            "module app; const Size: Int = 1; func main() { const : Int = 1; let = 2; }",
        );
        let facts = fixture.context();
        let (mut request, block) = crate::binding::test_support::request_and_block(&facts);
        let root = request.unit().root_scope();
        let mut operations = TestOperations::new(fixture.declared_type);

        let block = match request.bind_block(root, &block, &mut operations) {
            Ok(block) => block,
            Err(error) => panic!("malformed locals must recover: {error:?}"),
        };

        let result = match request.finish() {
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
        C: BinderFactContext + ?Sized,
    {
        fn bind_expression(
            &mut self,
            request: &mut BinderRequestContext<'_, C>,
            scope: bray_symbols::LocalScopeId,
            syntax: Option<&ExpressionSyntax>,
        ) -> BindingResult<bray_bound_tree::BoundExpressionId> {
            let values = request.unit().local_symbols_named(scope, "value")?.len();

            let constants = request.unit().local_symbols_named(scope, "Local")?.len();

            self.visible_names.push((values, constants));

            let Some(target) = request.control_target() else {
                panic!("block expressions must bind under a control target");
            };

            self.control_targets.push(target.kind());

            let origin = syntax.map_or_else(
                || BoundNodeOrigin::source(request.unit().key().source()),
                |syntax| request.source_origin(syntax),
            );

            let expression =
                BoundExpression::Error(BoundErrorExpression::new(origin, self.error_type));

            let expression = request
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
            request: &mut BinderRequestContext<'_, C>,
            scope: bray_symbols::LocalScopeId,
            _syntax: &GeneratorIterationExpressionSyntax,
        ) -> BindingResult<bray_bound_tree::BoundExpressionId> {
            self.bind_expression(request, scope, None)
        }

        fn bind_type_expression(
            &mut self,
            _request: &mut BinderRequestContext<'_, C>,
            _scope: bray_symbols::LocalScopeId,
            syntax: Option<&TypeExpressionSyntax>,
        ) -> BindingResult<Option<TypeId>> {
            Ok(syntax.map(|_| self.error_type))
        }

        fn error_type(&self) -> TypeId {
            self.error_type
        }
    }
}
