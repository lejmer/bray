use crate::node::{child_nodes, define_source_syntax_node};
use crate::{ArgumentListSyntax, SyntaxKind, TypeFormArgumentListSyntax};

define_source_syntax_node! {
    /// Type-form construction expression.
    pub struct TypeFormConstructionExpressionSyntax {
        builder: TypeFormConstructionExpressionSyntaxBuilder,
        kind: SyntaxKind::TypeFormConstructionExpression,
        source_slot: "type_form_construction_expression.source",
        node_name: "type form construction expression",
        range_description: "type-form-construction-expression",
        debug_name: "TypeFormConstructionExpressionSyntax",
        builder_debug_name: "TypeFormConstructionExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `box` keyword token.
                box_keyword;
                /// Appends the `box` keyword token.
                push_box_keyword;
                kind: SyntaxKind::BoxKeyword;
                slot: "type_form_construction_expression.box_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the construction argument-list child.
                argument_list;
                /// Appends the construction argument-list child.
                push_argument_list;
                ty: ArgumentListSyntax;
                kind: SyntaxKind::ArgumentList;
            }
        ],
        repeated_children: [
            {
                /// Returns type-form argument-list children in source order.
                type_form_argument_lists;
                /// Appends a type-form argument-list child.
                push_type_form_argument_list;
                ty: TypeFormArgumentListSyntax;
                kind: SyntaxKind::TypeFormArgumentList;
            }
        ],
    }
}

impl TypeFormConstructionExpressionSyntax {
    /// Returns the optional type-form argument list child.
    pub fn type_form_argument_list(&self) -> Option<TypeFormArgumentListSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::TypeFormArgumentList,
            TypeFormArgumentListSyntax::from_green,
        )
        .next()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token};
    use crate::{
        ArgumentListSyntax, SyntaxKind, SyntaxText, TypeFormArgumentListSyntax,
        TypeFormConstructionExpressionSyntax,
    };

    #[test]
    fn type_form_construction_expressions_store_arguments() {
        let snapshot = test_snapshot("syntax-type-form-construction-expression-test", "box[]()");

        let mut type_arguments =
            TypeFormArgumentListSyntax::builder(snapshot.clone(), TextSize::new(3));

        let mut arguments = ArgumentListSyntax::builder(snapshot.clone(), TextSize::new(5));
        let mut builder = TypeFormConstructionExpressionSyntax::builder(snapshot, TextSize::ZERO);

        type_arguments.push_open_bracket_token(token(SyntaxKind::OpenBracketToken, 3, 4));
        type_arguments.push_close_bracket_token(token(SyntaxKind::CloseBracketToken, 4, 5));

        arguments.push_open_paren_token(token(SyntaxKind::OpenParenToken, 5, 6));
        arguments.push_close_paren_token(token(SyntaxKind::CloseParenToken, 6, 7));

        builder.push_box_keyword(keyword(SyntaxKind::BoxKeyword, 0, 3, false));
        builder.push_type_form_argument_list(type_arguments.build());
        builder.push_argument_list(arguments.build());

        let expression = builder.build();

        assert_eq!(expression.full_text(), "box[]()");
        assert!(expression.type_form_argument_list().is_some());
    }
}
