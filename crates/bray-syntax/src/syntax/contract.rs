use crate::node::define_source_syntax_node;
use crate::{
    ExpressionSyntax, PathSyntax, SyntaxKind, SyntaxToken, TraitApplicationSyntax,
    TypeExpressionSyntax,
};

/// A trait-satisfaction constraint retained inside a static predicate expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraitSatisfactionConstraintSyntax {
    subject: TypeExpressionSyntax,
    colon_token: SyntaxToken,
    application: TraitApplicationSyntax,
}

/// One operand of a static type-equality constraint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaticTypeOperandSyntax {
    /// An operand parsed unambiguously as a type expression.
    Type(TypeExpressionSyntax),
    /// An operand whose expression syntax requires semantic classification.
    Expression(ExpressionSyntax),
}

/// A static constraint requiring two type expressions to denote the same type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeEqualityConstraintSyntax {
    left: StaticTypeOperandSyntax,
    equals_token: SyntaxToken,
    right: StaticTypeOperandSyntax,
}

impl TypeEqualityConstraintSyntax {
    /// Returns the left type operand.
    pub const fn left(&self) -> &StaticTypeOperandSyntax {
        &self.left
    }

    /// Returns the equality operator token.
    pub const fn equals_token(&self) -> &SyntaxToken {
        &self.equals_token
    }

    /// Returns the right type operand.
    pub const fn right(&self) -> &StaticTypeOperandSyntax {
        &self.right
    }
}

impl TraitSatisfactionConstraintSyntax {
    /// Returns the implementation-eligible subject type.
    pub const fn subject(&self) -> &TypeExpressionSyntax {
        &self.subject
    }

    /// Returns the separating colon token.
    pub const fn colon_token(&self) -> &SyntaxToken {
        &self.colon_token
    }

    /// Returns the exact required trait application.
    pub const fn application(&self) -> &TraitApplicationSyntax {
        &self.application
    }
}

impl ExpressionSyntax {
    /// Returns this expression as a direct trait-satisfaction constraint.
    pub fn trait_satisfaction_constraint(&self) -> Option<TraitSatisfactionConstraintSyntax> {
        let mut subjects = self.type_expressions();
        let mut applications = self.trait_applications();

        let subject = subjects.next()?;
        let application = applications.next()?;
        let colon_token = self.operator_token()?;

        if subjects.next().is_some()
            || applications.next().is_some()
            || colon_token.kind() != SyntaxKind::ColonToken
        {
            return None;
        }

        Some(TraitSatisfactionConstraintSyntax {
            subject,
            colon_token,
            application,
        })
    }

    /// Returns this expression as a possible static type-equality constraint.
    pub fn type_equality_constraint(&self) -> Option<TypeEqualityConstraintSyntax> {
        let equals_token = self.operator_token()?;

        if equals_token.kind() != SyntaxKind::EqualsEqualsToken {
            return None;
        }

        let types = self.type_expressions().collect::<Vec<_>>();
        let expressions = self.expressions().collect::<Vec<_>>();

        let (left, right) = match (types.as_slice(), expressions.as_slice()) {
            ([left, right], []) => (
                StaticTypeOperandSyntax::Type(left.clone()),
                StaticTypeOperandSyntax::Type(right.clone()),
            ),
            ([], [left, right]) => (
                StaticTypeOperandSyntax::Expression(left.clone()),
                StaticTypeOperandSyntax::Expression(right.clone()),
            ),
            _ => return None,
        };

        Some(TypeEqualityConstraintSyntax {
            left,
            equals_token,
            right,
        })
    }
}

macro_rules! define_expression_contract_clause_syntax {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            keyword_getter: $keyword_getter:ident,
            keyword_push: $keyword_push:ident,
            keyword_kind: $keyword_kind:path,
            keyword_slot: $keyword_slot:literal,
            open_paren_slot: $open_paren_slot:literal,
            comma_slot: $comma_slot:literal,
            close_paren_slot: $close_paren_slot:literal $(,)?
        }
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [
                    {
                        /// Returns the required clause keyword token.
                        $keyword_getter;
                        /// Appends the clause keyword token.
                        $keyword_push;
                        kind: $keyword_kind;
                        slot: $keyword_slot;
                    },
                    {
                        /// Returns the required opening parenthesis token.
                        open_paren_token;
                        /// Appends the opening parenthesis token.
                        push_open_paren_token;
                        kind: SyntaxKind::OpenParenToken;
                        slot: $open_paren_slot;
                    },
                    {
                        /// Returns the required closing parenthesis token.
                        close_paren_token;
                        /// Appends the closing parenthesis token.
                        push_close_paren_token;
                        kind: SyntaxKind::CloseParenToken;
                        slot: $close_paren_slot;
                    }
                ],
                optional_tokens: [
                    {
                        /// Returns the first comma separator token.
                        comma_token;
                        /// Appends a comma separator token.
                        push_separator_token;
                        kind: SyntaxKind::CommaToken;
                        slot: $comma_slot;
                    }
                ],
                required_children: [],
                repeated_children: [
                    {
                        /// Returns predicate expressions in source order.
                        expressions;
                        /// Appends a predicate expression.
                        push_expression;
                        ty: ExpressionSyntax;
                        kind: SyntaxKind::Expression;
                    }
                ],
            }
        }

        impl crate::node::GreenSeparatedSyntaxNode for $node_syntax {}

    };
}

define_expression_contract_clause_syntax! {
    /// `requires(...)` callable contract clause.
    RequiresClauseSyntax {
        builder: RequiresClauseSyntaxBuilder,
        kind: SyntaxKind::RequiresClause,
        source_slot: "requires_clause.source",
        node_name: "requires clause",
        range_description: "requires-clause",
        debug_name: "RequiresClauseSyntax",
        builder_debug_name: "RequiresClauseSyntaxBuilder",
        keyword_getter: requires_keyword,
        keyword_push: push_requires_keyword,
        keyword_kind: SyntaxKind::RequiresKeyword,
        keyword_slot: "requires_clause.requires_keyword",
        open_paren_slot: "requires_clause.open_paren_token",
        comma_slot: "requires_clause.comma_token",
        close_paren_slot: "requires_clause.close_paren_token",
    }
}

define_expression_contract_clause_syntax! {
    /// `ensures(...)` callable contract clause.
    EnsuresClauseSyntax {
        builder: EnsuresClauseSyntaxBuilder,
        kind: SyntaxKind::EnsuresClause,
        source_slot: "ensures_clause.source",
        node_name: "ensures clause",
        range_description: "ensures-clause",
        debug_name: "EnsuresClauseSyntax",
        builder_debug_name: "EnsuresClauseSyntaxBuilder",
        keyword_getter: ensures_keyword,
        keyword_push: push_ensures_keyword,
        keyword_kind: SyntaxKind::EnsuresKeyword,
        keyword_slot: "ensures_clause.ensures_keyword",
        open_paren_slot: "ensures_clause.open_paren_token",
        comma_slot: "ensures_clause.comma_token",
        close_paren_slot: "ensures_clause.close_paren_token",
    }
}

define_expression_contract_clause_syntax! {
    /// `with(...)` static constraint clause.
    WithClauseSyntax {
        builder: WithClauseSyntaxBuilder,
        kind: SyntaxKind::WithClause,
        source_slot: "with_clause.source",
        node_name: "with clause",
        range_description: "with-clause",
        debug_name: "WithClauseSyntax",
        builder_debug_name: "WithClauseSyntaxBuilder",
        keyword_getter: with_keyword,
        keyword_push: push_with_keyword,
        keyword_kind: SyntaxKind::WithKeyword,
        keyword_slot: "with_clause.with_keyword",
        open_paren_slot: "with_clause.open_paren_token",
        comma_slot: "with_clause.comma_token",
        close_paren_slot: "with_clause.close_paren_token",
    }
}

define_source_syntax_node! {
    /// `uses(...)` trusted capability clause.
    pub struct UsesClauseSyntax {
        builder: UsesClauseSyntaxBuilder,
        kind: SyntaxKind::UsesClause,
        source_slot: "uses_clause.source",
        node_name: "uses clause",
        range_description: "uses-clause",
        debug_name: "UsesClauseSyntax",
        builder_debug_name: "UsesClauseSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `uses` keyword token.
                uses_keyword;
                /// Appends the `uses` keyword token.
                push_uses_keyword;
                kind: SyntaxKind::UsesKeyword;
                slot: "uses_clause.uses_keyword";
            },
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "uses_clause.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "uses_clause.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "uses_clause.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns trusted capability paths in source order.
                paths;
                /// Appends a trusted capability path.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for UsesClauseSyntax {}

#[cfg(test)]
mod tests {
    use crate::SeparatedSyntaxNode;
    use bray_source::TextSize;

    use crate::test_support::{
        identifier_path, snapshot as test_snapshot, token, token_expression,
    };
    use crate::{RequiresClauseSyntax, SyntaxKind, SyntaxText, UsesClauseSyntax};

    #[test]
    fn expression_contract_clauses_store_expressions_and_separators() {
        let snapshot = test_snapshot("syntax-contract-test", "requires(valid,ready,)");
        let mut builder = RequiresClauseSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_requires_keyword(token(SyntaxKind::RequiresKeyword, 0, 8));
        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 8, 9));

        builder.push_expression(token_expression(
            snapshot.clone(),
            SyntaxKind::IdentifierToken,
            9,
            14,
        ));

        builder.push_separator_token(token(SyntaxKind::CommaToken, 14, 15));

        builder.push_expression(token_expression(
            snapshot,
            SyntaxKind::IdentifierToken,
            15,
            20,
        ));

        builder.push_separator_token(token(SyntaxKind::CommaToken, 20, 21));

        builder.push_close_paren_token(token(SyntaxKind::CloseParenToken, 21, 22));

        let clause = builder.build();

        assert_eq!(clause.full_text(), "requires(valid,ready,)");
        assert_eq!(clause.expressions().count(), 2);
        assert_eq!(clause.separator_tokens().count(), 2);
    }

    #[test]
    fn uses_clauses_store_capability_paths() {
        let snapshot = test_snapshot("syntax-contract-test", "uses(core,io)");
        let mut builder = UsesClauseSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_uses_keyword(token(SyntaxKind::UsesKeyword, 0, 4));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 4, 5));

        builder.push_path(identifier_path(snapshot.clone(), 5, 9));
        builder.push_separator_token(token(SyntaxKind::CommaToken, 9, 10));
        builder.push_path(identifier_path(snapshot, 10, 12));

        builder.push_close_paren_token(token(SyntaxKind::CloseParenToken, 12, 13));

        let clause = builder.build();

        assert_eq!(clause.full_text(), "uses(core,io)");
        assert_eq!(clause.paths().count(), 2);
        assert_eq!(clause.separator_tokens().count(), 1);
    }
}
