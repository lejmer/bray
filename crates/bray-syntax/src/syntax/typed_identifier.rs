use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken, TypeExpressionSyntax};

define_source_syntax_node! {
    /// Required type annotation on a named declaration item.
    pub struct TypeAnnotationSyntax {
        builder: TypeAnnotationSyntaxBuilder,
        kind: SyntaxKind::TypeAnnotation,
        source_slot: "type_annotation.source",
        node_name: "type annotation",
        range_description: "type-annotation",
        debug_name: "TypeAnnotationSyntax",
        builder_debug_name: "TypeAnnotationSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required colon token.
                colon_token;
                /// Appends the colon token.
                push_colon_token;
                kind: SyntaxKind::ColonToken;
                slot: "type_annotation.colon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the annotated type-expression child.
                type_expression;
                /// Appends the annotated type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Identifier with a required type annotation.
    pub struct TypedIdentifierSyntax {
        builder: TypedIdentifierSyntaxBuilder,
        kind: SyntaxKind::TypedIdentifier,
        source_slot: "typed_identifier.source",
        node_name: "typed identifier",
        range_description: "typed-identifier",
        debug_name: "TypedIdentifierSyntax",
        builder_debug_name: "TypedIdentifierSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required identifier token.
                identifier_token;
                /// Appends the identifier token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "typed_identifier.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the required type-annotation child.
                type_annotation;
                /// Appends the type-annotation child.
                push_type_annotation;
                ty: TypeAnnotationSyntax;
                kind: SyntaxKind::TypeAnnotation;
            }
        ],
    }
}

impl TypedIdentifierSyntax {
    /// Returns the colon token from this identifier's type annotation.
    pub fn colon_token(&self) -> SyntaxToken {
        self.type_annotation().colon_token()
    }

    /// Returns the type-expression child from this identifier's type annotation.
    pub fn type_expression(&self) -> TypeExpressionSyntax {
        self.type_annotation().type_expression()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{snapshot as test_snapshot, typed_identifier};
    use crate::{SyntaxKind, SyntaxText};

    #[test]
    fn typed_identifiers_store_identifier_annotation_and_type() {
        let snapshot = test_snapshot("syntax-typed-identifier-test", "value: Int");
        let typed_identifier = typed_identifier(snapshot, 0, 5, 7, 10, false);

        assert_eq!(typed_identifier.full_text(), "value: Int");

        assert_eq!(
            typed_identifier.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(
            typed_identifier.colon_token().kind(),
            SyntaxKind::ColonToken
        );

        assert_eq!(typed_identifier.type_annotation().full_text(), ": Int");
        assert_eq!(typed_identifier.type_expression().full_text(), "Int");
    }
}
