use crate::node::define_source_syntax_node;
use crate::{
    ExpressionSyntax, SyntaxKind, SyntaxToken, TypeExpressionSyntax, TypedIdentifierSyntax,
};

define_source_syntax_node! {
    /// Generic type parameter.
    pub struct GenericTypeParameterSyntax {
        builder: GenericTypeParameterSyntaxBuilder,
        kind: SyntaxKind::GenericTypeParameter,
        source_slot: "generic_type_parameter.source",
        node_name: "generic type parameter",
        range_description: "generic-type-parameter",
        debug_name: "GenericTypeParameterSyntax",
        builder_debug_name: "GenericTypeParameterSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required type parameter name token.
                identifier_token;
                /// Appends the type parameter name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "generic_type_parameter.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Generic const parameter.
    pub struct GenericConstParameterSyntax {
        builder: GenericConstParameterSyntaxBuilder,
        kind: SyntaxKind::GenericConstParameter,
        source_slot: "generic_const_parameter.source",
        node_name: "generic const parameter",
        range_description: "generic-const-parameter",
        debug_name: "GenericConstParameterSyntax",
        builder_debug_name: "GenericConstParameterSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required `const` keyword token.
                const_keyword;
                /// Appends the `const` keyword token.
                push_const_keyword;
                kind: SyntaxKind::ConstKeyword;
                slot: "generic_const_parameter.const_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the typed-identifier child.
                typed_identifier;
                /// Appends the typed-identifier child.
                push_typed_identifier;
                ty: TypedIdentifierSyntax;
                kind: SyntaxKind::TypedIdentifier;
            }
        ],
    }
}

impl GenericConstParameterSyntax {
    /// Returns the required const parameter name token.
    pub fn identifier_token(&self) -> SyntaxToken {
        self.typed_identifier().identifier_token()
    }
}

define_source_syntax_node! {
    /// Generic parameter list including delimiters.
    pub struct GenericParameterListSyntax {
        builder: GenericParameterListSyntaxBuilder,
        kind: SyntaxKind::GenericParameterList,
        source_slot: "generic_parameter_list.source",
        node_name: "generic parameter list",
        range_description: "generic-parameter-list",
        debug_name: "GenericParameterListSyntax",
        builder_debug_name: "GenericParameterListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening angle token.
                less_token;
                /// Appends the opening angle token.
                push_less_token;
                kind: SyntaxKind::LessToken;
                slot: "generic_parameter_list.less_token";
            },
            {
                /// Returns the required closing angle token.
                greater_token;
                /// Appends the closing angle token.
                push_greater_token;
                kind: SyntaxKind::GreaterToken;
                slot: "generic_parameter_list.greater_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "generic_parameter_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns generic type parameters in source order.
                generic_type_parameters;
                /// Appends a generic type parameter.
                push_generic_type_parameter;
                ty: GenericTypeParameterSyntax;
                kind: SyntaxKind::GenericTypeParameter;
            },
            {
                /// Returns generic const parameters in source order.
                generic_const_parameters;
                /// Appends a generic const parameter.
                push_generic_const_parameter;
                ty: GenericConstParameterSyntax;
                kind: SyntaxKind::GenericConstParameter;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for GenericParameterListSyntax {}

define_source_syntax_node! {
    /// Generic argument.
    pub struct GenericArgumentSyntax {
        builder: GenericArgumentSyntaxBuilder,
        kind: SyntaxKind::GenericArgument,
        source_slot: "generic_argument.source",
        node_name: "generic argument",
        range_description: "generic-argument",
        debug_name: "GenericArgumentSyntax",
        builder_debug_name: "GenericArgumentSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns type-expression children in source order.
                type_expressions;
                /// Appends a type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            },
            {
                /// Returns constant-expression children in source order.
                expressions;
                /// Appends a constant-expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Generic argument list including delimiters.
    pub struct GenericArgumentListSyntax {
        builder: GenericArgumentListSyntaxBuilder,
        kind: SyntaxKind::GenericArgumentList,
        source_slot: "generic_argument_list.source",
        node_name: "generic argument list",
        range_description: "generic-argument-list",
        debug_name: "GenericArgumentListSyntax",
        builder_debug_name: "GenericArgumentListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening angle token.
                less_token;
                /// Appends the opening angle token.
                push_less_token;
                kind: SyntaxKind::LessToken;
                slot: "generic_argument_list.less_token";
            },
            {
                /// Returns the required closing angle token.
                greater_token;
                /// Appends the closing angle token.
                push_greater_token;
                kind: SyntaxKind::GreaterToken;
                slot: "generic_argument_list.greater_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "generic_argument_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns generic arguments in source order.
                generic_arguments;
                /// Appends a generic argument.
                push_generic_argument;
                ty: GenericArgumentSyntax;
                kind: SyntaxKind::GenericArgument;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for GenericArgumentListSyntax {}

define_source_syntax_node! {
    /// Type-form argument.
    pub struct TypeFormArgumentSyntax {
        builder: TypeFormArgumentSyntaxBuilder,
        kind: SyntaxKind::TypeFormArgument,
        source_slot: "type_form_argument.source",
        node_name: "type form argument",
        range_description: "type-form-argument",
        debug_name: "TypeFormArgumentSyntax",
        builder_debug_name: "TypeFormArgumentSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns type-expression children in source order.
                type_expressions;
                /// Appends a type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            },
            {
                /// Returns constant-expression children in source order.
                expressions;
                /// Appends a constant-expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Type-form argument list including delimiters.
    pub struct TypeFormArgumentListSyntax {
        builder: TypeFormArgumentListSyntaxBuilder,
        kind: SyntaxKind::TypeFormArgumentList,
        source_slot: "type_form_argument_list.source",
        node_name: "type form argument list",
        range_description: "type-form-argument-list",
        debug_name: "TypeFormArgumentListSyntax",
        builder_debug_name: "TypeFormArgumentListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "type_form_argument_list.open_bracket_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "type_form_argument_list.close_bracket_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "type_form_argument_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns type-form arguments in source order.
                type_form_arguments;
                /// Appends a type-form argument.
                push_type_form_argument;
                ty: TypeFormArgumentSyntax;
                kind: SyntaxKind::TypeFormArgument;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for TypeFormArgumentListSyntax {}

#[cfg(test)]
mod tests {
    use crate::SeparatedSyntaxNode;
    use bray_source::TextSize;

    use super::{
        GenericArgumentListSyntax, GenericArgumentSyntax, GenericConstParameterSyntax,
        GenericParameterListSyntax, GenericTypeParameterSyntax, TypeFormArgumentListSyntax,
        TypeFormArgumentSyntax,
    };
    use crate::test_support::{
        identifier_type_expression, keyword, snapshot as test_snapshot, token, token_expression,
        typed_identifier,
    };
    use crate::{SyntaxKind, SyntaxText};

    #[test]
    fn generic_parameter_lists_store_type_const_and_separator_children() {
        let snapshot = test_snapshot("syntax-generic-test", "<T, const N: Int,>");

        let mut type_parameter =
            GenericTypeParameterSyntax::builder(snapshot.clone(), TextSize::new(1));

        type_parameter.push_identifier_token(token(SyntaxKind::IdentifierToken, 1, 2));

        let mut const_parameter =
            GenericConstParameterSyntax::builder(snapshot.clone(), TextSize::new(4));

        const_parameter.push_const_keyword(keyword(SyntaxKind::ConstKeyword, 4, 9, true));

        const_parameter.push_typed_identifier(typed_identifier(
            snapshot.clone(),
            10,
            11,
            13,
            16,
            false,
        ));

        let mut list = GenericParameterListSyntax::builder(snapshot, TextSize::ZERO);

        list.push_less_token(token(SyntaxKind::LessToken, 0, 1));

        list.push_generic_type_parameter(type_parameter.build());
        list.push_separator_token(keyword(SyntaxKind::CommaToken, 2, 3, true));
        list.push_generic_const_parameter(const_parameter.build());
        list.push_separator_token(token(SyntaxKind::CommaToken, 16, 17));

        list.push_greater_token(token(SyntaxKind::GreaterToken, 17, 18));

        let list = list.build();

        assert_eq!(list.full_text(), "<T, const N: Int,>");
        assert_eq!(list.generic_type_parameters().count(), 1);
        assert_eq!(list.generic_const_parameters().count(), 1);
        assert_eq!(list.separator_tokens().count(), 2);

        let const_parameter = match list.generic_const_parameters().next() {
            Some(parameter) => parameter,
            None => panic!("expected generic const parameter"),
        };

        assert_eq!(
            const_parameter.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(
            const_parameter
                .typed_identifier()
                .type_expression()
                .full_text(),
            "Int"
        );
    }

    #[test]
    fn generic_argument_lists_store_type_and_constant_arguments() {
        let snapshot = test_snapshot("syntax-generic-test", "<T, 1>");
        let mut type_argument = GenericArgumentSyntax::builder(snapshot.clone(), TextSize::new(1));

        type_argument.push_type_expression(identifier_type_expression(snapshot.clone(), 1, 2));

        let mut constant_argument =
            GenericArgumentSyntax::builder(snapshot.clone(), TextSize::new(4));

        constant_argument.push_expression(token_expression(
            snapshot.clone(),
            SyntaxKind::DecimalIntegerLiteralToken,
            4,
            5,
        ));

        let mut list = GenericArgumentListSyntax::builder(snapshot, TextSize::ZERO);

        list.push_less_token(token(SyntaxKind::LessToken, 0, 1));

        list.push_generic_argument(type_argument.build());
        list.push_separator_token(keyword(SyntaxKind::CommaToken, 2, 3, true));
        list.push_generic_argument(constant_argument.build());

        list.push_greater_token(token(SyntaxKind::GreaterToken, 5, 6));

        let list = list.build();
        let arguments = list.generic_arguments().collect::<Vec<_>>();

        let [type_argument, constant_argument] = arguments.as_slice() else {
            panic!("expected two generic arguments: {arguments:?}");
        };

        assert_eq!(list.full_text(), "<T, 1>");
        assert_eq!(list.separator_tokens().count(), 1);
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);
    }

    #[test]
    fn type_form_argument_lists_store_type_and_constant_arguments() {
        let snapshot = test_snapshot("syntax-generic-test", "[T, 1]");
        let mut type_argument = TypeFormArgumentSyntax::builder(snapshot.clone(), TextSize::new(1));

        type_argument.push_type_expression(identifier_type_expression(snapshot.clone(), 1, 2));

        let mut constant_argument =
            TypeFormArgumentSyntax::builder(snapshot.clone(), TextSize::new(4));

        constant_argument.push_expression(token_expression(
            snapshot.clone(),
            SyntaxKind::DecimalIntegerLiteralToken,
            4,
            5,
        ));

        let mut list = TypeFormArgumentListSyntax::builder(snapshot, TextSize::ZERO);

        list.push_open_bracket_token(token(SyntaxKind::OpenBracketToken, 0, 1));

        list.push_type_form_argument(type_argument.build());
        list.push_separator_token(keyword(SyntaxKind::CommaToken, 2, 3, true));
        list.push_type_form_argument(constant_argument.build());

        list.push_close_bracket_token(token(SyntaxKind::CloseBracketToken, 5, 6));

        let list = list.build();
        let arguments = list.type_form_arguments().collect::<Vec<_>>();

        let [type_argument, constant_argument] = arguments.as_slice() else {
            panic!("expected two type-form arguments: {arguments:?}");
        };

        assert_eq!(list.full_text(), "[T, 1]");
        assert_eq!(list.separator_tokens().count(), 1);
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);
    }
}
