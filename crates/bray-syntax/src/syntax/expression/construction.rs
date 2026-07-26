use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Struct field initializer entry.
    pub struct StructFieldInitializerSyntax {
        builder: StructFieldInitializerSyntaxBuilder,
        kind: SyntaxKind::StructFieldInitializer,
        source_slot: "struct_field_initializer.source",
        node_name: "struct field initializer",
        range_description: "struct-field-initializer",
        debug_name: "StructFieldInitializerSyntax",
        builder_debug_name: "StructFieldInitializerSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required field name token.
                identifier_token;
                /// Appends the field name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "struct_field_initializer.identifier_token";
            },
            {
                /// Returns the required equals token.
                equals_token;
                /// Appends the equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "struct_field_initializer.equals_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the field initializer expression child.
                expression;
                /// Appends the field initializer expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Struct construction body including delimiters.
    pub struct StructConstructionBodySyntax {
        builder: StructConstructionBodySyntaxBuilder,
        kind: SyntaxKind::StructConstructionBody,
        source_slot: "struct_construction_body.source",
        node_name: "struct construction body",
        range_description: "struct-construction-body",
        debug_name: "StructConstructionBodySyntax",
        builder_debug_name: "StructConstructionBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "struct_construction_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "struct_construction_body.close_brace_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "struct_construction_body.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns field initializers in source order.
                field_initializers;
                /// Appends a field initializer.
                push_field_initializer;
                ty: StructFieldInitializerSyntax;
                kind: SyntaxKind::StructFieldInitializer;
            }
        ],
    }
}

impl StructConstructionBodySyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token, token_expression};
    use crate::{
        StructConstructionBodySyntax, StructFieldInitializerSyntax, SyntaxKind, SyntaxText,
    };

    #[test]
    fn struct_construction_bodies_store_typed_children() {
        let snapshot = test_snapshot("syntax-struct-construction-test", "{ x = 1}");
        let expression_source = snapshot.clone();

        let mut initializer =
            StructFieldInitializerSyntax::builder(snapshot.clone(), TextSize::new(2));

        let mut body = StructConstructionBodySyntax::builder(snapshot, TextSize::ZERO);

        initializer.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 2, 3, true));
        initializer.push_equals_token(keyword(SyntaxKind::EqualsToken, 4, 5, true));

        initializer.push_expression(token_expression(
            expression_source,
            SyntaxKind::DecimalIntegerLiteralToken,
            6,
            7,
        ));

        body.push_open_brace_token(keyword(SyntaxKind::OpenBraceToken, 0, 1, true));
        body.push_field_initializer(initializer.build());
        body.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 7, 8));

        assert_eq!(body.build().full_text(), "{ x = 1}");
    }
}
