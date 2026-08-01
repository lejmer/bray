use super::expression::first_expression;
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    ConstantDeclarationSyntax, ExpressionSyntax, GeneratorIterationExpressionSyntax,
    IrrefutablePatternSyntax, SyntaxKind, TypeAnnotationSyntax,
};

define_source_syntax_node! {
    /// Braced block expression.
    pub struct BlockExpressionSyntax {
        builder: BlockExpressionSyntaxBuilder,
        kind: SyntaxKind::BlockExpression,
        source_slot: "block_expression.source",
        node_name: "block expression",
        range_description: "block-expression",
        debug_name: "BlockExpressionSyntax",
        builder_debug_name: "BlockExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "block_expression.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "block_expression.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns block-item children in source order.
                block_items;
                /// Appends a block-item child.
                push_block_item;
                ty: BlockItemSyntax;
                kind: SyntaxKind::BlockItem;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Item inside a block expression.
    pub struct BlockItemSyntax {
        builder: BlockItemSyntaxBuilder,
        kind: SyntaxKind::BlockItem,
        source_slot: "block_item.source",
        node_name: "block item",
        range_description: "block-item",
        debug_name: "BlockItemSyntax",
        builder_debug_name: "BlockItemSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns local binding declaration children in source order.
                local_binding_declarations;
                /// Appends a local binding declaration child.
                push_local_binding_declaration;
                ty: LocalBindingDeclarationSyntax;
                kind: SyntaxKind::LocalBindingDeclaration;
            },
            {
                /// Returns constant declaration children in source order.
                constant_declarations;
                /// Appends a constant declaration child.
                push_constant_declaration;
                ty: ConstantDeclarationSyntax;
                kind: SyntaxKind::ConstantDeclaration;
            },
            {
                /// Returns sequenced-expression children in source order.
                sequenced_expressions;
                /// Appends a sequenced-expression child.
                push_sequenced_expression;
                ty: SequencedExpressionSyntax;
                kind: SyntaxKind::SequencedExpression;
            },
            {
                /// Returns unterminated block-shaped expressions in source order.
                block_shaped_expressions;
                /// Appends an unterminated block-shaped expression.
                push_block_shaped_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            },
            {
                /// Returns generator-iteration-expression children in source order.
                generator_iteration_expressions;
                /// Appends a generator-iteration-expression child.
                push_generator_iteration_expression;
                ty: GeneratorIterationExpressionSyntax;
                kind: SyntaxKind::GeneratorIterationExpression;
            }
        ],
    }
}

impl BlockItemSyntax {
    /// Returns the local binding declaration child when present.
    pub fn local_binding_declaration(&self) -> Option<LocalBindingDeclarationSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::LocalBindingDeclaration,
            LocalBindingDeclarationSyntax::from_green,
        )
        .next()
    }

    /// Returns the constant declaration child when present.
    pub fn constant_declaration(&self) -> Option<ConstantDeclarationSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::ConstantDeclaration,
            ConstantDeclarationSyntax::from_green,
        )
        .next()
    }

    /// Returns the sequenced expression child when present.
    pub fn sequenced_expression(&self) -> Option<SequencedExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::SequencedExpression,
            SequencedExpressionSyntax::from_green,
        )
        .next()
    }

    /// Returns the unterminated block-shaped expression child when present.
    pub fn block_shaped_expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }

    /// Returns the expression represented directly or through a sequenced expression.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        self.block_shaped_expression().or_else(|| {
            self.sequenced_expression()
                .and_then(|sequence| sequence.expression())
        })
    }

    /// Returns the generator iteration expression child when present.
    pub fn generator_iteration_expression(&self) -> Option<GeneratorIterationExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::GeneratorIterationExpression,
            GeneratorIterationExpressionSyntax::from_green,
        )
        .next()
    }
}

define_source_syntax_node! {
    /// Local binding declaration.
    pub struct LocalBindingDeclarationSyntax {
        builder: LocalBindingDeclarationSyntaxBuilder,
        kind: SyntaxKind::LocalBindingDeclaration,
        source_slot: "local_binding_declaration.source",
        node_name: "local binding declaration",
        range_description: "local-binding-declaration",
        debug_name: "LocalBindingDeclarationSyntax",
        builder_debug_name: "LocalBindingDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `let` keyword token.
                let_keyword;
                /// Appends the `let` keyword token.
                push_let_keyword;
                kind: SyntaxKind::LetKeyword;
                slot: "local_binding_declaration.let_keyword";
            },
            {
                /// Returns the required initializer equals token.
                equals_token;
                /// Appends the initializer equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "local_binding_declaration.equals_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "local_binding_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the required irrefutable pattern child.
                irrefutable_pattern;
                /// Appends the irrefutable pattern child.
                push_irrefutable_pattern;
                ty: IrrefutablePatternSyntax;
                kind: SyntaxKind::IrrefutablePattern;
            }
        ],
        repeated_children: [
            {
                /// Returns type annotation children in source order.
                type_annotations;
                /// Appends a type annotation child.
                push_type_annotation;
                ty: TypeAnnotationSyntax;
                kind: SyntaxKind::TypeAnnotation;
            },
            {
                /// Returns initializer expression children in source order.
                expressions;
                /// Appends an initializer expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl LocalBindingDeclarationSyntax {
    /// Returns the optional type annotation child.
    pub fn type_annotation(&self) -> Option<TypeAnnotationSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::TypeAnnotation,
            TypeAnnotationSyntax::from_green,
        )
        .next()
    }

    /// Returns the initializer expression child.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

define_source_syntax_node! {
    /// Expression sequenced by a semicolon.
    pub struct SequencedExpressionSyntax {
        builder: SequencedExpressionSyntaxBuilder,
        kind: SyntaxKind::SequencedExpression,
        source_slot: "sequenced_expression.source",
        node_name: "sequenced expression",
        range_description: "sequenced-expression",
        debug_name: "SequencedExpressionSyntax",
        builder_debug_name: "SequencedExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "sequenced_expression.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns expression children in source order.
                expressions;
                /// Appends an expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl SequencedExpressionSyntax {
    /// Returns the expression child.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextSize};

    use crate::test_support::{
        identifier_path_with_trailing_space, keyword, snapshot as test_snapshot, token,
        token_expression, typed_identifier,
    };
    use crate::{
        BlockExpressionSyntax, BlockItemSyntax, ConstantDeclarationSyntax, ConstantModifiersSyntax,
        IrrefutablePatternSyntax, LocalBindingDeclarationSyntax, SequencedExpressionSyntax,
        SyntaxKind, SyntaxText,
    };

    #[test]
    fn block_expressions_store_items_in_source_order() {
        let snapshot = test_snapshot("syntax-block-test", "{ let value = 1; const X: Int = 2; }");

        let mut block = BlockExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        block.push_open_brace_token(keyword(SyntaxKind::OpenBraceToken, 0, 1, true));
        block.push_block_item(local_binding_item(snapshot.clone()));
        block.push_block_item(constant_item(snapshot));
        block.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 35, 36));

        let block = block.build();
        let items = block.block_items().collect::<Vec<_>>();

        let [local, constant] = items.as_slice() else {
            panic!("expected local and constant block items: {items:?}");
        };

        assert_eq!(block.full_text(), "{ let value = 1; const X: Int = 2; }");
        assert!(local.local_binding_declaration().is_some());
        assert!(constant.constant_declaration().is_some());
    }

    #[test]
    fn sequenced_expressions_store_expression_and_semicolon() {
        let snapshot = test_snapshot("syntax-block-test", "value;");

        let mut sequence = SequencedExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        sequence.push_expression(identifier_expression(snapshot));
        sequence.push_semicolon_token(token(SyntaxKind::SemicolonToken, 5, 6));

        let sequence = sequence.build();

        assert_eq!(sequence.full_text(), "value;");

        assert_eq!(
            sequence
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value"))
        );
    }

    fn local_binding_item(snapshot: SourceSnapshot) -> BlockItemSyntax {
        let mut pattern = IrrefutablePatternSyntax::builder(snapshot.clone(), TextSize::new(6));

        let mut declaration =
            LocalBindingDeclarationSyntax::builder(snapshot.clone(), TextSize::new(2));

        let mut item = BlockItemSyntax::builder(snapshot.clone(), TextSize::new(2));

        pattern.push_path(identifier_path_with_trailing_space(snapshot.clone(), 6, 11));

        declaration.push_let_keyword(keyword(SyntaxKind::LetKeyword, 2, 5, true));
        declaration.push_irrefutable_pattern(pattern.build());
        declaration.push_equals_token(keyword(SyntaxKind::EqualsToken, 12, 13, true));

        declaration.push_expression(token_expression(
            snapshot,
            SyntaxKind::DecimalIntegerLiteralToken,
            14,
            15,
        ));

        declaration.push_semicolon_token(keyword(SyntaxKind::SemicolonToken, 15, 16, true));

        item.push_local_binding_declaration(declaration.build());

        item.build()
    }

    fn constant_item(snapshot: SourceSnapshot) -> BlockItemSyntax {
        let mut declaration =
            ConstantDeclarationSyntax::builder(snapshot.clone(), TextSize::new(17));

        let mut item = BlockItemSyntax::builder(snapshot.clone(), TextSize::new(17));

        declaration.push_constant_modifiers(
            ConstantModifiersSyntax::builder(snapshot.clone(), TextSize::new(17)).build(),
        );

        declaration.push_const_keyword(keyword(SyntaxKind::ConstKeyword, 17, 22, true));
        declaration.push_typed_identifier(typed_identifier(snapshot.clone(), 23, 24, 26, 29, true));
        declaration.push_equals_token(keyword(SyntaxKind::EqualsToken, 30, 31, true));

        declaration.push_expression(token_expression(
            snapshot,
            SyntaxKind::DecimalIntegerLiteralToken,
            32,
            33,
        ));

        declaration.push_semicolon_token(keyword(SyntaxKind::SemicolonToken, 33, 34, true));

        item.push_constant_declaration(declaration.build());

        item.build()
    }

    fn identifier_expression(snapshot: SourceSnapshot) -> crate::ExpressionSyntax {
        token_expression(snapshot, SyntaxKind::IdentifierToken, 0, 5)
    }
}
