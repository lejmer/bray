use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    CallableResultClauseSyntax, ExpressionSyntax, GenericArgumentListSyntax, ParameterListSyntax,
    PathSyntax, SyntaxKind, SyntaxToken, TypeFormArgumentListSyntax,
};

define_source_syntax_node! {
    /// Type expression.
    pub struct TypeExpressionSyntax {
        builder: TypeExpressionSyntaxBuilder,
        kind: SyntaxKind::TypeExpression,
        source_slot: "type_expression.source",
        node_name: "type expression",
        range_description: "type-expression",
        debug_name: "TypeExpressionSyntax",
        builder_debug_name: "TypeExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional borrow ampersand token.
                ampersand_token;
                /// Appends a borrow ampersand token.
                push_ampersand_token;
                kind: SyntaxKind::AmpersandToken;
                slot: "type_expression.ampersand_token";
            },
            {
                /// Returns the optional borrow `mut` token.
                mut_token;
                /// Appends a borrow `mut` token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "type_expression.mut_token";
            },
            {
                /// Returns the optional `box` keyword token.
                box_keyword;
                /// Appends a `box` keyword token.
                push_box_keyword;
                kind: SyntaxKind::BoxKeyword;
                slot: "type_expression.box_keyword";
            },
            {
                /// Returns the optional `func` keyword token.
                func_keyword;
                /// Appends a `func` keyword token.
                push_func_keyword;
                kind: SyntaxKind::FuncKeyword;
                slot: "type_expression.func_keyword";
            },
            {
                /// Returns the optional `Self` keyword token.
                self_keyword;
                /// Appends a `Self` keyword token.
                push_self_keyword;
                kind: SyntaxKind::SelfTypeKeyword;
                slot: "type_expression.self_keyword";
            },
            {
                /// Returns the optional opening parenthesis token.
                open_paren_token;
                /// Appends an opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "type_expression.open_paren_token";
            },
            {
                /// Returns the optional closing parenthesis token.
                close_paren_token;
                /// Appends a closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "type_expression.close_paren_token";
            },
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "type_expression.comma_token";
            },
            {
                /// Returns the optional opening bracket token.
                open_bracket_token;
                /// Appends an opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "type_expression.open_bracket_token";
            },
            {
                /// Returns the optional array-size semicolon token.
                semicolon_token;
                /// Appends an array-size semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "type_expression.semicolon_token";
            },
            {
                /// Returns the optional closing bracket token.
                close_bracket_token;
                /// Appends a closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "type_expression.close_bracket_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns direct nested type-expression children in source order.
                type_expressions;
                /// Appends a nested type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            },
            {
                /// Returns direct type-form argument lists in source order.
                type_form_argument_lists;
                /// Appends a type-form argument list child.
                push_type_form_argument_list;
                ty: TypeFormArgumentListSyntax;
                kind: SyntaxKind::TypeFormArgumentList;
            },
            {
                /// Returns direct generic argument lists in source order.
                generic_argument_lists;
                /// Appends a generic argument list child.
                push_generic_argument_list;
                ty: GenericArgumentListSyntax;
                kind: SyntaxKind::GenericArgumentList;
            },
            {
                /// Returns direct path children in source order.
                paths;
                /// Appends a path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            },
            {
                /// Returns direct callable parameter-list children in source order.
                parameter_lists;
                /// Appends a callable parameter-list child.
                push_parameter_list;
                ty: ParameterListSyntax;
                kind: SyntaxKind::ParameterList;
            },
            {
                /// Returns direct callable result-clause children in source order.
                callable_result_clauses;
                /// Appends a callable result-clause child.
                push_callable_result_clause;
                ty: CallableResultClauseSyntax;
                kind: SyntaxKind::CallableResultClause;
            },
            {
                /// Returns direct runtime expression children in source order.
                expressions;
                /// Appends a runtime expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl TypeExpressionSyntax {
    /// Returns the direct path child when this is a path type expression.
    pub fn path(&self) -> Option<PathSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::Path,
            PathSyntax::from_green,
        )
        .next()
    }

    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use crate::test_support::{identifier_path, keyword, snapshot as test_snapshot, token};
    use crate::{PathSyntax, SyntaxKind, SyntaxText, SyntaxTrivia, TypeExpressionSyntax};

    #[test]
    fn type_expressions_store_path_children() {
        let snapshot = test_snapshot("syntax-type-expression-test", "core.Int");

        let mut path = PathSyntax::builder(snapshot.clone());
        let mut builder = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        path.push_identifier_token(token(SyntaxKind::IdentifierToken, 0, 4));
        path.push_dot_token(token(SyntaxKind::DotToken, 4, 5));
        path.push_identifier_token(token(SyntaxKind::IdentifierToken, 5, 8));
        builder.push_path(path.build());

        let expression = builder.build();

        assert_eq!(expression.full_text(), "core.Int");

        assert_eq!(
            expression.path().map(|path| path.full_text()),
            Some(String::from("core.Int"))
        );
    }

    #[test]
    fn type_expressions_store_prefix_and_nested_children() {
        let snapshot = test_snapshot("syntax-type-expression-test", "&mut Value");
        let mut child = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::new(5));

        child.push_path(identifier_path(snapshot.clone(), 5, 10));

        let mut builder = TypeExpressionSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_ampersand_token(token(SyntaxKind::AmpersandToken, 0, 1));
        builder.push_mut_token(keyword(SyntaxKind::MutKeyword, 1, 4, true));
        builder.push_type_expression(child.build());

        let expression = builder.build();

        assert_eq!(expression.full_text(), "&mut Value");
        assert_eq!(expression.type_expressions().count(), 1);
    }

    #[test]
    fn type_expressions_store_tuple_separators_and_skipped_recovery() {
        let snapshot = test_snapshot("syntax-type-expression-test", "(A, B<T>)");

        let mut first = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::new(1));
        let mut second = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::new(4));
        let mut builder = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        first.push_path(identifier_path(snapshot.clone(), 1, 2));
        second.push_path(identifier_path(snapshot, 4, 5));

        second.push_skipped_tokens([
            token(SyntaxKind::LessToken, 5, 6),
            token(SyntaxKind::IdentifierToken, 6, 7),
            token(SyntaxKind::GreaterToken, 7, 8),
        ]);

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 0, 1));
        builder.push_type_expression(first.build());
        builder.push_separator_token(keyword(SyntaxKind::CommaToken, 2, 3, true));
        builder.push_type_expression(second.build());
        builder.push_close_paren_token(token(SyntaxKind::CloseParenToken, 8, 9));

        let expression = builder.build();

        assert_eq!(expression.full_text(), "(A, B<T>)");
        assert_eq!(expression.type_expressions().count(), 2);
        assert_eq!(expression.separator_tokens().count(), 1);
        assert_eq!(expression.skipped_syntax().count(), 1);
    }

    #[test]
    fn type_expressions_store_callable_type_children() {
        let snapshot = test_snapshot("syntax-type-expression-test", "func() -> Value");

        let mut parameters =
            crate::ParameterListSyntax::builder(snapshot.clone(), TextSize::new(4));

        let mut result =
            crate::CallableResultClauseSyntax::builder(snapshot.clone(), TextSize::new(7));

        let mut result_type = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::new(10));
        let mut builder = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        parameters.push_open_paren_token(token(SyntaxKind::OpenParenToken, 4, 5));
        parameters.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 5, 6, true));

        result_type.push_path(identifier_path(snapshot, 10, 15));

        result.push_arrow_token(token(SyntaxKind::ArrowToken, 7, 9).with_trailing_trivia([
            SyntaxTrivia::whitespace(TextRange::new(TextSize::new(9), TextSize::new(10))),
        ]));

        result.push_type_expression(result_type.build());

        builder.push_func_keyword(token(SyntaxKind::FuncKeyword, 0, 4));
        builder.push_parameter_list(parameters.build());
        builder.push_callable_result_clause(result.build());

        let expression = builder.build();

        assert_eq!(expression.full_text(), "func() -> Value");
        assert_eq!(expression.parameter_lists().count(), 1);
        assert_eq!(expression.callable_result_clauses().count(), 1);
    }
}
