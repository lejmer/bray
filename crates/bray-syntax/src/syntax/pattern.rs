use crate::node::define_source_syntax_node;
use crate::{PathSyntax, SyntaxKind, SyntaxToken};

macro_rules! define_pattern_root_syntax {
    (
        $(#[$node_meta:meta])*
        $node:ident {
            builder: $builder:ident,
            kind: $kind:path,
            source_slot: $source_slot:literal,
            slot_prefix: $slot_prefix:literal,
            slots: {
                discard: $discard_slot:literal,
                mut_keyword: $mut_keyword_slot:literal,
                none_keyword: $none_keyword_slot:literal,
                question: $question_slot:literal,
                box_keyword: $box_keyword_slot:literal,
                open_paren: $open_paren_slot:literal,
                close_paren: $close_paren_slot:literal,
                dot: $dot_slot:literal,
                identifier: $identifier_slot:literal,
                open_bracket: $open_bracket_slot:literal,
                close_bracket: $close_bracket_slot:literal,
                open_brace: $open_brace_slot:literal,
                close_brace: $close_brace_slot:literal,
                comma: $comma_slot:literal,
                dot_dot: $dot_dot_slot:literal $(,)?
            },
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            child_accessor: $child_accessor:ident,
            child_pusher: $child_pusher:ident,
            child_ty: $child_ty:ident,
            child_kind: $child_kind:path,
            entry_accessor: $entry_accessor:ident,
            entry_pusher: $entry_pusher:ident,
            entry_ty: $entry_ty:ident,
            entry_kind: $entry_kind:path,
            extra_optional_tokens: [$($extra_optional_tokens:tt)*] $(,)?
        }
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node {
                builder: $builder,
                kind: $kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [],
                optional_tokens: [
                    {
                        /// Returns the optional discard pattern token.
                        discard_token;
                        /// Appends a discard pattern token.
                        push_discard_token;
                        kind: SyntaxKind::UnderscoreToken;
                        slot: $discard_slot;
                    },
                    {
                        /// Returns the optional `mut` binding token.
                        mut_keyword;
                        /// Appends a `mut` binding token.
                        push_mut_keyword;
                        kind: SyntaxKind::MutKeyword;
                        slot: $mut_keyword_slot;
                    },
                    {
                        /// Returns the optional `none` token.
                        none_keyword;
                        /// Appends a `none` token.
                        push_none_keyword;
                        kind: SyntaxKind::NoneKeyword;
                        slot: $none_keyword_slot;
                    },
                    {
                        /// Returns the optional nullable-present question token.
                        question_token;
                        /// Appends a nullable-present question token.
                        push_question_token;
                        kind: SyntaxKind::QuestionToken;
                        slot: $question_slot;
                    },
                    {
                        /// Returns the optional `box` token.
                        box_keyword;
                        /// Appends a `box` token.
                        push_box_keyword;
                        kind: SyntaxKind::BoxKeyword;
                        slot: $box_keyword_slot;
                    },
                    {
                        /// Returns the optional opening parenthesis token.
                        open_paren_token;
                        /// Appends an opening parenthesis token.
                        push_open_paren_token;
                        kind: SyntaxKind::OpenParenToken;
                        slot: $open_paren_slot;
                    },
                    {
                        /// Returns the optional closing parenthesis token.
                        close_paren_token;
                        /// Appends a closing parenthesis token.
                        push_close_paren_token;
                        kind: SyntaxKind::CloseParenToken;
                        slot: $close_paren_slot;
                    },
                    {
                        /// Returns the optional leading-dot token.
                        dot_token;
                        /// Appends a leading-dot token.
                        push_dot_token;
                        kind: SyntaxKind::DotToken;
                        slot: $dot_slot;
                    },
                    {
                        /// Returns the optional identifier token.
                        identifier_token;
                        /// Appends an identifier token.
                        push_identifier_token;
                        kind: SyntaxKind::IdentifierToken;
                        slot: $identifier_slot;
                    },
                    {
                        /// Returns the optional opening bracket token.
                        open_bracket_token;
                        /// Appends an opening bracket token.
                        push_open_bracket_token;
                        kind: SyntaxKind::OpenBracketToken;
                        slot: $open_bracket_slot;
                    },
                    {
                        /// Returns the optional closing bracket token.
                        close_bracket_token;
                        /// Appends a closing bracket token.
                        push_close_bracket_token;
                        kind: SyntaxKind::CloseBracketToken;
                        slot: $close_bracket_slot;
                    },
                    {
                        /// Returns the optional opening brace token.
                        open_brace_token;
                        /// Appends an opening brace token.
                        push_open_brace_token;
                        kind: SyntaxKind::OpenBraceToken;
                        slot: $open_brace_slot;
                    },
                    {
                        /// Returns the optional closing brace token.
                        close_brace_token;
                        /// Appends a closing brace token.
                        push_close_brace_token;
                        kind: SyntaxKind::CloseBraceToken;
                        slot: $close_brace_slot;
                    },
                    {
                        /// Returns the first comma separator token.
                        comma_token;
                        /// Appends a comma separator token.
                        push_separator_token;
                        kind: SyntaxKind::CommaToken;
                        slot: $comma_slot;
                    },
                    {
                        /// Returns the optional remaining-pattern token.
                        dot_dot_token;
                        /// Appends a remaining-pattern token.
                        push_dot_dot_token;
                        kind: SyntaxKind::DotDotToken;
                        slot: $dot_dot_slot;
                    }
                    $($extra_optional_tokens)*
                ],
                required_children: [],
                repeated_children: [
                    {
                        /// Returns direct path children in source order.
                        paths;
                        /// Appends a path child.
                        push_path;
                        ty: PathSyntax;
                        kind: SyntaxKind::Path;
                    },
                    {
                        /// Returns direct nested pattern children in source order.
                        $child_accessor;
                        /// Appends a nested pattern child.
                        $child_pusher;
                        ty: $child_ty;
                        kind: $child_kind;
                    },
                    {
                        /// Returns direct pattern entry children in source order.
                        $entry_accessor;
                        /// Appends a pattern entry child.
                        $entry_pusher;
                        ty: $entry_ty;
                        kind: $entry_kind;
                    }
                ],
            }
        }

        impl crate::node::GreenSeparatedSyntaxNode for $node {}

        impl $node {
            /// Returns the first literal-pattern token.
            pub fn literal_token(&self) -> Option<SyntaxToken> {
                self.node.first_child_token_matching(self.start, SyntaxKind::is_pattern_literal)
            }

        }

        impl $builder {
            /// Appends a literal-pattern token.
            pub fn push_literal_token(&mut self, token: SyntaxToken) {
                assert!(
                    token.kind().is_pattern_literal(),
                    concat!($slot_prefix, ".literal_token expected a literal-pattern token")
                );

                self.node.push_token(token);
            }
        }
    };
}

macro_rules! define_pattern_entry_syntax {
    (
        $(#[$node_meta:meta])*
        $node:ident {
            builder: $builder:ident,
            kind: $kind:path,
            source_slot: $source_slot:literal,
            slots: {
                identifier: $identifier_slot:literal,
                equals: $equals_slot:literal,
                dot_dot: $dot_dot_slot:literal $(,)?
            },
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            child_accessor: $child_accessor:ident,
            child_pusher: $child_pusher:ident,
            child_ty: $child_ty:ident,
            child_kind: $child_kind:path $(,)?
        }
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node {
                builder: $builder,
                kind: $kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [],
                optional_tokens: [
                    {
                        /// Returns the optional entry name token.
                        identifier_token;
                        /// Appends an entry name token.
                        push_identifier_token;
                        kind: SyntaxKind::IdentifierToken;
                        slot: $identifier_slot;
                    },
                    {
                        /// Returns the optional named-entry equals token.
                        equals_token;
                        /// Appends a named-entry equals token.
                        push_equals_token;
                        kind: SyntaxKind::EqualsToken;
                        slot: $equals_slot;
                    },
                    {
                        /// Returns the optional remaining-pattern token.
                        dot_dot_token;
                        /// Appends a remaining-pattern token.
                        push_dot_dot_token;
                        kind: SyntaxKind::DotDotToken;
                        slot: $dot_dot_slot;
                    }
                ],
                required_children: [],
                repeated_children: [
                    {
                        /// Returns direct nested pattern children in source order.
                        $child_accessor;
                        /// Appends a nested pattern child.
                        $child_pusher;
                        ty: $child_ty;
                        kind: $child_kind;
                    }
                ],
            }
        }
    };
}

define_pattern_root_syntax! {
    /// Irrefutable pattern.
    IrrefutablePatternSyntax {
        builder: IrrefutablePatternSyntaxBuilder,
        kind: SyntaxKind::IrrefutablePattern,
        source_slot: "irrefutable_pattern.source",
        slot_prefix: "irrefutable_pattern",
        slots: {
            discard: "irrefutable_pattern.discard_token",
            mut_keyword: "irrefutable_pattern.mut_keyword",
            none_keyword: "irrefutable_pattern.none_keyword",
            question: "irrefutable_pattern.question_token",
            box_keyword: "irrefutable_pattern.box_keyword",
            open_paren: "irrefutable_pattern.open_paren_token",
            close_paren: "irrefutable_pattern.close_paren_token",
            dot: "irrefutable_pattern.dot_token",
            identifier: "irrefutable_pattern.identifier_token",
            open_bracket: "irrefutable_pattern.open_bracket_token",
            close_bracket: "irrefutable_pattern.close_bracket_token",
            open_brace: "irrefutable_pattern.open_brace_token",
            close_brace: "irrefutable_pattern.close_brace_token",
            comma: "irrefutable_pattern.comma_token",
            dot_dot: "irrefutable_pattern.dot_dot_token",
        },
        node_name: "irrefutable pattern",
        range_description: "irrefutable-pattern",
        debug_name: "IrrefutablePatternSyntax",
        builder_debug_name: "IrrefutablePatternSyntaxBuilder",
        child_accessor: irrefutable_patterns,
        child_pusher: push_irrefutable_pattern,
        child_ty: IrrefutablePatternSyntax,
        child_kind: SyntaxKind::IrrefutablePattern,
        entry_accessor: irrefutable_pattern_entries,
        entry_pusher: push_irrefutable_pattern_entry,
        entry_ty: IrrefutablePatternEntrySyntax,
        entry_kind: SyntaxKind::IrrefutablePatternEntry,
        extra_optional_tokens: []
    }
}

define_pattern_entry_syntax! {
    /// Entry inside an irrefutable pattern body.
    IrrefutablePatternEntrySyntax {
        builder: IrrefutablePatternEntrySyntaxBuilder,
        kind: SyntaxKind::IrrefutablePatternEntry,
        source_slot: "irrefutable_pattern_entry.source",
        slots: {
            identifier: "irrefutable_pattern_entry.identifier_token",
            equals: "irrefutable_pattern_entry.equals_token",
            dot_dot: "irrefutable_pattern_entry.dot_dot_token",
        },
        node_name: "irrefutable pattern entry",
        range_description: "irrefutable-pattern-entry",
        debug_name: "IrrefutablePatternEntrySyntax",
        builder_debug_name: "IrrefutablePatternEntrySyntaxBuilder",
        child_accessor: irrefutable_patterns,
        child_pusher: push_irrefutable_pattern,
        child_ty: IrrefutablePatternSyntax,
        child_kind: SyntaxKind::IrrefutablePattern
    }
}

define_pattern_root_syntax! {
    /// Case pattern.
    CasePatternSyntax {
        builder: CasePatternSyntaxBuilder,
        kind: SyntaxKind::CasePattern,
        source_slot: "case_pattern.source",
        slot_prefix: "case_pattern",
        slots: {
            discard: "case_pattern.discard_token",
            mut_keyword: "case_pattern.mut_keyword",
            none_keyword: "case_pattern.none_keyword",
            question: "case_pattern.question_token",
            box_keyword: "case_pattern.box_keyword",
            open_paren: "case_pattern.open_paren_token",
            close_paren: "case_pattern.close_paren_token",
            dot: "case_pattern.dot_token",
            identifier: "case_pattern.identifier_token",
            open_bracket: "case_pattern.open_bracket_token",
            close_bracket: "case_pattern.close_bracket_token",
            open_brace: "case_pattern.open_brace_token",
            close_brace: "case_pattern.close_brace_token",
            comma: "case_pattern.comma_token",
            dot_dot: "case_pattern.dot_dot_token",
        },
        node_name: "case pattern",
        range_description: "case-pattern",
        debug_name: "CasePatternSyntax",
        builder_debug_name: "CasePatternSyntaxBuilder",
        child_accessor: case_patterns,
        child_pusher: push_case_pattern,
        child_ty: CasePatternSyntax,
        child_kind: SyntaxKind::CasePattern,
        entry_accessor: case_pattern_entries,
        entry_pusher: push_case_pattern_entry,
        entry_ty: CasePatternEntrySyntax,
        entry_kind: SyntaxKind::CasePatternEntry,
        extra_optional_tokens: [
            ,
            {
                /// Returns the first alternative separator token.
                pipe_token;
                /// Appends an alternative separator token.
                push_pipe_token;
                kind: SyntaxKind::PipeToken;
                slot: "case_pattern.pipe_token";
            }
        ]
    }
}

impl CasePatternSyntax {
    /// Returns alternative separator tokens in source order.
    pub fn alternative_separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node
            .child_tokens(self.start)
            .filter(|token| token.kind() == SyntaxKind::PipeToken)
    }
}

define_pattern_entry_syntax! {
    /// Entry inside a case pattern body.
    CasePatternEntrySyntax {
        builder: CasePatternEntrySyntaxBuilder,
        kind: SyntaxKind::CasePatternEntry,
        source_slot: "case_pattern_entry.source",
        slots: {
            identifier: "case_pattern_entry.identifier_token",
            equals: "case_pattern_entry.equals_token",
            dot_dot: "case_pattern_entry.dot_dot_token",
        },
        node_name: "case pattern entry",
        range_description: "case-pattern-entry",
        debug_name: "CasePatternEntrySyntax",
        builder_debug_name: "CasePatternEntrySyntaxBuilder",
        child_accessor: case_patterns,
        child_pusher: push_case_pattern,
        child_ty: CasePatternSyntax,
        child_kind: SyntaxKind::CasePattern
    }
}

#[cfg(test)]
mod tests {
    use crate::SeparatedSyntaxNode;
    use bray_source::TextSize;

    use crate::test_support::{
        identifier_path, identifier_path_with_trailing_space, keyword, snapshot as test_snapshot,
        token,
    };
    use crate::{
        CasePatternEntrySyntax, CasePatternSyntax, IrrefutablePatternEntrySyntax,
        IrrefutablePatternSyntax, SyntaxKind, SyntaxText,
    };

    #[test]
    fn irrefutable_patterns_store_product_entries() {
        let snapshot = test_snapshot("syntax-pattern-test", "Point { x = mut value, .. }");

        let mut pattern = IrrefutablePatternSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut field = IrrefutablePatternEntrySyntax::builder(snapshot.clone(), TextSize::new(8));
        let mut nested = IrrefutablePatternSyntax::builder(snapshot.clone(), TextSize::new(12));

        let mut remaining =
            IrrefutablePatternEntrySyntax::builder(snapshot.clone(), TextSize::new(23));

        nested.push_mut_keyword(keyword(SyntaxKind::MutKeyword, 12, 15, true));
        nested.push_identifier_token(token(SyntaxKind::IdentifierToken, 16, 21));

        field.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 8, 9, true));
        field.push_equals_token(keyword(SyntaxKind::EqualsToken, 10, 11, true));
        field.push_irrefutable_pattern(nested.build());

        remaining.push_dot_dot_token(keyword(SyntaxKind::DotDotToken, 23, 25, true));

        pattern.push_path(identifier_path_with_trailing_space(snapshot, 0, 5));
        pattern.push_open_brace_token(keyword(SyntaxKind::OpenBraceToken, 6, 7, true));
        pattern.push_irrefutable_pattern_entry(field.build());
        pattern.push_separator_token(keyword(SyntaxKind::CommaToken, 21, 22, true));
        pattern.push_irrefutable_pattern_entry(remaining.build());
        pattern.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 26, 27));

        let pattern = pattern.build();

        assert_eq!(pattern.full_text(), "Point { x = mut value, .. }");
        assert_eq!(pattern.paths().count(), 1);
        assert_eq!(pattern.irrefutable_pattern_entries().count(), 2);
        assert_eq!(pattern.separator_tokens().count(), 1);
    }

    #[test]
    fn case_patterns_store_alternatives_and_payload_entries() {
        let snapshot = test_snapshot("syntax-pattern-test", "Some(value) | none");

        let mut root = CasePatternSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut some = CasePatternSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut entry = CasePatternEntrySyntax::builder(snapshot.clone(), TextSize::new(5));
        let mut value = CasePatternSyntax::builder(snapshot.clone(), TextSize::new(5));
        let mut none = CasePatternSyntax::builder(snapshot.clone(), TextSize::new(14));

        value.push_path(identifier_path(snapshot.clone(), 5, 10));

        entry.push_case_pattern(value.build());

        some.push_path(identifier_path(snapshot, 0, 4));
        some.push_open_paren_token(token(SyntaxKind::OpenParenToken, 4, 5));
        some.push_case_pattern_entry(entry.build());
        some.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 10, 11, true));

        none.push_none_keyword(token(SyntaxKind::NoneKeyword, 14, 18));

        root.push_case_pattern(some.build());
        root.push_pipe_token(keyword(SyntaxKind::PipeToken, 12, 13, true));
        root.push_case_pattern(none.build());

        let pattern = root.build();

        assert_eq!(pattern.full_text(), "Some(value) | none");
        assert_eq!(pattern.case_patterns().count(), 2);
        assert_eq!(pattern.alternative_separator_tokens().count(), 1);
    }

    #[test]
    fn pattern_literal_tokens_accept_boolean_keywords() {
        let snapshot = test_snapshot("syntax-pattern-test", "true");
        let mut pattern = CasePatternSyntax::builder(snapshot, TextSize::ZERO);

        pattern.push_literal_token(token(SyntaxKind::TrueKeyword, 0, 4));

        let pattern = pattern.build();

        assert_eq!(
            pattern.literal_token().map(|token| token.kind()),
            Some(SyntaxKind::TrueKeyword)
        );
    }
}
