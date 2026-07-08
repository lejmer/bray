use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    BlockExpressionSyntax, SyntaxKind, SyntaxToken, TraitApplicationSyntax, TypeExpressionSyntax,
};

pub(super) fn first_expression(
    source: &bray_source::SourceSnapshot,
    node: &crate::green::GreenNode,
    start: bray_source::TextSize,
) -> Option<ExpressionSyntax> {
    child_nodes(
        source,
        node,
        start,
        SyntaxKind::Expression,
        ExpressionSyntax::from_green,
    )
    .next()
}

define_source_syntax_node! {
    /// Runtime expression.
    pub struct ExpressionSyntax {
        builder: ExpressionSyntaxBuilder,
        kind: SyntaxKind::Expression,
        source_slot: "expression.source",
        node_name: "expression",
        range_description: "expression",
        debug_name: "ExpressionSyntax",
        builder_debug_name: "ExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns direct nested expression children in source order.
                expressions;
                /// Appends a nested expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            },
            {
                /// Returns direct primary-expression children in source order.
                primary_expressions;
                /// Appends a primary-expression child.
                push_primary_expression;
                ty: PrimaryExpressionSyntax;
                kind: SyntaxKind::PrimaryExpression;
            },
            {
                /// Returns member-access postfix operations in source order.
                member_access_operations;
                /// Appends a member-access postfix operation.
                push_member_access_operation;
                ty: MemberAccessOperationSyntax;
                kind: SyntaxKind::MemberAccessOperation;
            },
            {
                /// Returns element-index postfix operations in source order.
                element_index_operations;
                /// Appends an element-index postfix operation.
                push_element_index_operation;
                ty: ElementIndexOperationSyntax;
                kind: SyntaxKind::ElementIndexOperation;
            },
            {
                /// Returns call postfix operations in source order.
                call_operations;
                /// Appends a call postfix operation.
                push_call_operation;
                ty: CallOperationSyntax;
                kind: SyntaxKind::CallOperation;
            },
            {
                /// Returns slice-index postfix operations in source order.
                slice_index_operations;
                /// Appends a slice-index postfix operation.
                push_slice_index_operation;
                ty: SliceIndexOperationSyntax;
                kind: SyntaxKind::SliceIndexOperation;
            },
            {
                /// Returns nullable-propagation postfix operations in source order.
                nullable_propagation_operations;
                /// Appends a nullable-propagation postfix operation.
                push_nullable_propagation_operation;
                ty: NullablePropagationOperationSyntax;
                kind: SyntaxKind::NullablePropagationOperation;
            },
            {
                /// Returns conversion postfix operations in source order.
                conversion_operations;
                /// Appends a conversion postfix operation.
                push_conversion_operation;
                ty: ConversionOperationSyntax;
                kind: SyntaxKind::ConversionOperation;
            },
            {
                /// Returns trait-qualified member postfix operations in source order.
                trait_qualified_member_operations;
                /// Appends a trait-qualified member postfix operation.
                push_trait_qualified_member_operation;
                ty: TraitQualifiedMemberOperationSyntax;
                kind: SyntaxKind::TraitQualifiedMemberOperation;
            }
        ],
    }
}

impl ExpressionSyntax {
    /// Returns the operator token when this expression is operator-shaped.
    pub fn operator_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| is_expression_operator(token.kind()))
    }

    /// Returns the primary-expression child when this is a primary expression.
    pub fn primary_expression(&self) -> Option<PrimaryExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::PrimaryExpression,
            PrimaryExpressionSyntax::from_green,
        )
        .next()
    }
}

impl ExpressionSyntaxBuilder {
    /// Appends an expression operator token.
    pub fn push_operator_token(&mut self, token: SyntaxToken) {
        assert!(
            is_expression_operator(token.kind()),
            "expression.operator_token expected an expression operator"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Primary expression wrapper.
    pub struct PrimaryExpressionSyntax {
        builder: PrimaryExpressionSyntaxBuilder,
        kind: SyntaxKind::PrimaryExpression,
        source_slot: "primary_expression.source",
        node_name: "primary expression",
        range_description: "primary-expression",
        debug_name: "PrimaryExpressionSyntax",
        builder_debug_name: "PrimaryExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns access-expression children in source order.
                access_expressions;
                /// Appends an access-expression child.
                push_access_expression;
                ty: AccessExpressionSyntax;
                kind: SyntaxKind::AccessExpression;
            },
            {
                /// Returns struct-construction bodies in source order.
                struct_construction_bodies;
                /// Appends a struct-construction body child.
                push_struct_construction_body;
                ty: StructConstructionBodySyntax;
                kind: SyntaxKind::StructConstructionBody;
            },
            {
                /// Returns literal-expression children in source order.
                literal_expressions;
                /// Appends a literal-expression child.
                push_literal_expression;
                ty: LiteralExpressionSyntax;
                kind: SyntaxKind::LiteralExpression;
            },
            {
                /// Returns unit-expression children in source order.
                unit_expressions;
                /// Appends a unit-expression child.
                push_unit_expression;
                ty: UnitExpressionSyntax;
                kind: SyntaxKind::UnitExpression;
            },
            {
                /// Returns absence-expression children in source order.
                absence_expressions;
                /// Appends an absence-expression child.
                push_absence_expression;
                ty: AbsenceExpressionSyntax;
                kind: SyntaxKind::AbsenceExpression;
            },
            {
                /// Returns grouped-expression children in source order.
                grouped_expressions;
                /// Appends a grouped-expression child.
                push_grouped_expression;
                ty: GroupedExpressionSyntax;
                kind: SyntaxKind::GroupedExpression;
            },
            {
                /// Returns tuple-expression children in source order.
                tuple_expressions;
                /// Appends a tuple-expression child.
                push_tuple_expression;
                ty: TupleExpressionSyntax;
                kind: SyntaxKind::TupleExpression;
            },
            {
                /// Returns array-expression children in source order.
                array_expressions;
                /// Appends an array-expression child.
                push_array_expression;
                ty: ArrayExpressionSyntax;
                kind: SyntaxKind::ArrayExpression;
            },
            {
                /// Returns leading-dot variant-expression children in source order.
                leading_dot_variant_expressions;
                /// Appends a leading-dot variant-expression child.
                push_leading_dot_variant_expression;
                ty: LeadingDotVariantExpressionSyntax;
                kind: SyntaxKind::LeadingDotVariantExpression;
            },
            {
                /// Returns block-expression children in source order.
                block_expressions;
                /// Appends a block-expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
    }
}

impl PrimaryExpressionSyntax {
    /// Returns the first direct primary token.
    pub fn primary_token(&self) -> Option<SyntaxToken> {
        self.tokens().next()
    }

    /// Returns the access-expression child when this is access-shaped.
    pub fn access_expression(&self) -> Option<AccessExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::AccessExpression,
            AccessExpressionSyntax::from_green,
        )
        .next()
    }

    /// Returns the block-expression child when this is a block primary.
    pub fn block_expression(&self) -> Option<BlockExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::BlockExpression,
            BlockExpressionSyntax::from_green,
        )
        .next()
    }

    /// Returns the struct-construction body child when present.
    pub fn struct_construction_body(&self) -> Option<StructConstructionBodySyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::StructConstructionBody,
            StructConstructionBodySyntax::from_green,
        )
        .next()
    }
}

impl PrimaryExpressionSyntaxBuilder {
    /// Appends a token owned by the primary-expression shell.
    pub fn push_token(&mut self, token: SyntaxToken) {
        assert!(
            is_primary_expression_token(token.kind()),
            "primary_expression.token expected a primary-expression token"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Access expression.
    pub struct AccessExpressionSyntax {
        builder: AccessExpressionSyntaxBuilder,
        kind: SyntaxKind::AccessExpression,
        source_slot: "access_expression.source",
        node_name: "access expression",
        range_description: "access-expression",
        debug_name: "AccessExpressionSyntax",
        builder_debug_name: "AccessExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the identifier root token.
                identifier_token;
                /// Appends the identifier root token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "access_expression.identifier_token";
            },
            {
                /// Returns the `self` root token.
                self_token;
                /// Appends the `self` root token.
                push_self_token;
                kind: SyntaxKind::SelfValueKeyword;
                slot: "access_expression.self_token";
            },
            {
                /// Returns the `internal` root token.
                internal_token;
                /// Appends the `internal` root token.
                push_internal_token;
                kind: SyntaxKind::InternalKeyword;
                slot: "access_expression.internal_token";
            },
            {
                /// Returns the grouped-access opening parenthesis token.
                open_paren_token;
                /// Appends the grouped-access opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "access_expression.open_paren_token";
            },
            {
                /// Returns the grouped-access closing parenthesis token.
                close_paren_token;
                /// Appends the grouped-access closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "access_expression.close_paren_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns nested access-expression children in source order.
                access_expressions;
                /// Appends a nested access-expression child.
                push_access_expression;
                ty: AccessExpressionSyntax;
                kind: SyntaxKind::AccessExpression;
            },
            {
                /// Returns member-access operations in source order.
                member_access_operations;
                /// Appends a member-access operation.
                push_member_access_operation;
                ty: MemberAccessOperationSyntax;
                kind: SyntaxKind::MemberAccessOperation;
            },
            {
                /// Returns element-index operations in source order.
                element_index_operations;
                /// Appends an element-index operation.
                push_element_index_operation;
                ty: ElementIndexOperationSyntax;
                kind: SyntaxKind::ElementIndexOperation;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Member access postfix operation.
    pub struct MemberAccessOperationSyntax {
        builder: MemberAccessOperationSyntaxBuilder,
        kind: SyntaxKind::MemberAccessOperation,
        source_slot: "member_access_operation.source",
        node_name: "member access operation",
        range_description: "member-access-operation",
        debug_name: "MemberAccessOperationSyntax",
        builder_debug_name: "MemberAccessOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required dot token.
                dot_token;
                /// Appends the dot token.
                push_dot_token;
                kind: SyntaxKind::DotToken;
                slot: "member_access_operation.dot_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the member identifier token.
                identifier_token;
                /// Appends the member identifier token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "member_access_operation.identifier_token";
            },
            {
                /// Returns the tuple-element index token.
                tuple_element_index_token;
                /// Appends the tuple-element index token.
                push_tuple_element_index_token;
                kind: SyntaxKind::TupleElementIndexToken;
                slot: "member_access_operation.tuple_element_index_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Element index postfix operation.
    pub struct ElementIndexOperationSyntax {
        builder: ElementIndexOperationSyntaxBuilder,
        kind: SyntaxKind::ElementIndexOperation,
        source_slot: "element_index_operation.source",
        node_name: "element index operation",
        range_description: "element-index-operation",
        debug_name: "ElementIndexOperationSyntax",
        builder_debug_name: "ElementIndexOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "element_index_operation.open_bracket_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "element_index_operation.close_bracket_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the index expression child.
                expression;
                /// Appends the index expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Call postfix operation.
    pub struct CallOperationSyntax {
        builder: CallOperationSyntaxBuilder,
        kind: SyntaxKind::CallOperation,
        source_slot: "call_operation.source",
        node_name: "call operation",
        range_description: "call-operation",
        debug_name: "CallOperationSyntax",
        builder_debug_name: "CallOperationSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the argument-list child.
                argument_list;
                /// Appends the argument-list child.
                push_argument_list;
                ty: ArgumentListSyntax;
                kind: SyntaxKind::ArgumentList;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Runtime argument entry.
    pub struct ArgumentSyntax {
        builder: ArgumentSyntaxBuilder,
        kind: SyntaxKind::Argument,
        source_slot: "argument.source",
        node_name: "argument",
        range_description: "argument",
        debug_name: "ArgumentSyntax",
        builder_debug_name: "ArgumentSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the named argument identifier token.
                identifier_token;
                /// Appends the named argument identifier token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "argument.identifier_token";
            },
            {
                /// Returns the named argument equals token.
                equals_token;
                /// Appends the named argument equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "argument.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the argument expression child.
                expression;
                /// Appends the argument expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Runtime argument list including delimiters.
    pub struct ArgumentListSyntax {
        builder: ArgumentListSyntaxBuilder,
        kind: SyntaxKind::ArgumentList,
        source_slot: "argument_list.source",
        node_name: "argument list",
        range_description: "argument-list",
        debug_name: "ArgumentListSyntax",
        builder_debug_name: "ArgumentListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "argument_list.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "argument_list.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "argument_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns argument entries in source order.
                arguments;
                /// Appends an argument entry.
                push_argument;
                ty: ArgumentSyntax;
                kind: SyntaxKind::Argument;
            }
        ],
    }
}

impl ArgumentListSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Slice index postfix operation.
    pub struct SliceIndexOperationSyntax {
        builder: SliceIndexOperationSyntaxBuilder,
        kind: SyntaxKind::SliceIndexOperation,
        source_slot: "slice_index_operation.source",
        node_name: "slice index operation",
        range_description: "slice-index-operation",
        debug_name: "SliceIndexOperationSyntax",
        builder_debug_name: "SliceIndexOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "slice_index_operation.open_bracket_token";
            },
            {
                /// Returns the required range token.
                dot_dot_token;
                /// Appends the range token.
                push_dot_dot_token;
                kind: SyntaxKind::DotDotToken;
                slot: "slice_index_operation.dot_dot_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "slice_index_operation.close_bracket_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns slice bound expressions in source order.
                expressions;
                /// Appends a slice bound expression.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Nullable propagation postfix operation.
    pub struct NullablePropagationOperationSyntax {
        builder: NullablePropagationOperationSyntaxBuilder,
        kind: SyntaxKind::NullablePropagationOperation,
        source_slot: "nullable_propagation_operation.source",
        node_name: "nullable propagation operation",
        range_description: "nullable-propagation-operation",
        debug_name: "NullablePropagationOperationSyntax",
        builder_debug_name: "NullablePropagationOperationSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required question token.
                question_token;
                /// Appends the question token.
                push_question_token;
                kind: SyntaxKind::QuestionToken;
                slot: "nullable_propagation_operation.question_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Conversion postfix operation.
    pub struct ConversionOperationSyntax {
        builder: ConversionOperationSyntaxBuilder,
        kind: SyntaxKind::ConversionOperation,
        source_slot: "conversion_operation.source",
        node_name: "conversion operation",
        range_description: "conversion-operation",
        debug_name: "ConversionOperationSyntax",
        builder_debug_name: "ConversionOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `as` keyword token.
                as_keyword;
                /// Appends the `as` keyword token.
                push_as_keyword;
                kind: SyntaxKind::AsKeyword;
                slot: "conversion_operation.as_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the target type-expression child.
                type_expression;
                /// Appends the target type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Trait-qualified member postfix operation.
    pub struct TraitQualifiedMemberOperationSyntax {
        builder: TraitQualifiedMemberOperationSyntaxBuilder,
        kind: SyntaxKind::TraitQualifiedMemberOperation,
        source_slot: "trait_qualified_member_operation.source",
        node_name: "trait-qualified member operation",
        range_description: "trait-qualified-member-operation",
        debug_name: "TraitQualifiedMemberOperationSyntax",
        builder_debug_name: "TraitQualifiedMemberOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "trait_qualified_member_operation.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "trait_qualified_member_operation.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the trait-application child.
                trait_application;
                /// Appends the trait-application child.
                push_trait_application;
                ty: TraitApplicationSyntax;
                kind: SyntaxKind::TraitApplication;
            },
            {
                /// Returns the member-access operation child.
                member_access_operation;
                /// Appends the member-access operation child.
                push_member_access_operation;
                ty: MemberAccessOperationSyntax;
                kind: SyntaxKind::MemberAccessOperation;
            }
        ],
    }
}

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

define_source_syntax_node! {
    /// Literal expression.
    pub struct LiteralExpressionSyntax {
        builder: LiteralExpressionSyntaxBuilder,
        kind: SyntaxKind::LiteralExpression,
        source_slot: "literal_expression.source",
        node_name: "literal expression",
        range_description: "literal-expression",
        debug_name: "LiteralExpressionSyntax",
        builder_debug_name: "LiteralExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl LiteralExpressionSyntax {
    /// Returns the literal token.
    pub fn literal_token(&self) -> Option<SyntaxToken> {
        self.tokens().next()
    }
}

impl LiteralExpressionSyntaxBuilder {
    /// Appends a literal token.
    pub fn push_literal_token(&mut self, token: SyntaxToken) {
        assert!(
            is_literal_expression_token(token.kind()),
            "literal_expression.literal_token expected a literal token"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Unit expression.
    pub struct UnitExpressionSyntax {
        builder: UnitExpressionSyntaxBuilder,
        kind: SyntaxKind::UnitExpression,
        source_slot: "unit_expression.source",
        node_name: "unit expression",
        range_description: "unit-expression",
        debug_name: "UnitExpressionSyntax",
        builder_debug_name: "UnitExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required `unit` keyword token.
                unit_keyword;
                /// Appends the `unit` keyword token.
                push_unit_keyword;
                kind: SyntaxKind::UnitKeyword;
                slot: "unit_expression.unit_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Absence expression.
    pub struct AbsenceExpressionSyntax {
        builder: AbsenceExpressionSyntaxBuilder,
        kind: SyntaxKind::AbsenceExpression,
        source_slot: "absence_expression.source",
        node_name: "absence expression",
        range_description: "absence-expression",
        debug_name: "AbsenceExpressionSyntax",
        builder_debug_name: "AbsenceExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required `none` keyword token.
                none_keyword;
                /// Appends the `none` keyword token.
                push_none_keyword;
                kind: SyntaxKind::NoneKeyword;
                slot: "absence_expression.none_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Parenthesized grouped expression.
    pub struct GroupedExpressionSyntax {
        builder: GroupedExpressionSyntaxBuilder,
        kind: SyntaxKind::GroupedExpression,
        source_slot: "grouped_expression.source",
        node_name: "grouped expression",
        range_description: "grouped-expression",
        debug_name: "GroupedExpressionSyntax",
        builder_debug_name: "GroupedExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "grouped_expression.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "grouped_expression.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the grouped expression child.
                expression;
                /// Appends the grouped expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Tuple expression.
    pub struct TupleExpressionSyntax {
        builder: TupleExpressionSyntaxBuilder,
        kind: SyntaxKind::TupleExpression,
        source_slot: "tuple_expression.source",
        node_name: "tuple expression",
        range_description: "tuple-expression",
        debug_name: "TupleExpressionSyntax",
        builder_debug_name: "TupleExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "tuple_expression.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "tuple_expression.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "tuple_expression.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns tuple element expressions in source order.
                expressions;
                /// Appends a tuple element expression.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl TupleExpressionSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Array expression.
    pub struct ArrayExpressionSyntax {
        builder: ArrayExpressionSyntaxBuilder,
        kind: SyntaxKind::ArrayExpression,
        source_slot: "array_expression.source",
        node_name: "array expression",
        range_description: "array-expression",
        debug_name: "ArrayExpressionSyntax",
        builder_debug_name: "ArrayExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "array_expression.open_bracket_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "array_expression.close_bracket_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "array_expression.comma_token";
            },
            {
                /// Returns the repeated-array semicolon token.
                semicolon_token;
                /// Appends the repeated-array semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "array_expression.semicolon_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns array expressions in source order.
                expressions;
                /// Appends an array expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl ArrayExpressionSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Leading-dot variant expression.
    pub struct LeadingDotVariantExpressionSyntax {
        builder: LeadingDotVariantExpressionSyntaxBuilder,
        kind: SyntaxKind::LeadingDotVariantExpression,
        source_slot: "leading_dot_variant_expression.source",
        node_name: "leading-dot variant expression",
        range_description: "leading-dot-variant-expression",
        debug_name: "LeadingDotVariantExpressionSyntax",
        builder_debug_name: "LeadingDotVariantExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required dot token.
                dot_token;
                /// Appends the dot token.
                push_dot_token;
                kind: SyntaxKind::DotToken;
                slot: "leading_dot_variant_expression.dot_token";
            },
            {
                /// Returns the required variant name token.
                identifier_token;
                /// Appends the variant name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "leading_dot_variant_expression.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

fn is_expression_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EqualsToken
            | SyntaxKind::PipePipeToken
            | SyntaxKind::AmpersandAmpersandToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::BangEqualsToken
            | SyntaxKind::LessToken
            | SyntaxKind::LessEqualsToken
            | SyntaxKind::GreaterToken
            | SyntaxKind::GreaterEqualsToken
            | SyntaxKind::PipeToken
            | SyntaxKind::CaretToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::LessLessToken
            | SyntaxKind::GreaterGreaterToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::StarToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::AtToken
            | SyntaxKind::StarStarToken
            | SyntaxKind::TildeToken
            | SyntaxKind::BangToken
    )
}

fn is_primary_expression_token(kind: SyntaxKind) -> bool {
    kind.is_token()
}

fn is_literal_expression_token(kind: SyntaxKind) -> bool {
    kind.is_literal() || matches!(kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token, token_expression};
    use crate::{
        AccessExpressionSyntax, ArgumentListSyntax, ArgumentSyntax, ExpressionSyntax,
        LiteralExpressionSyntax, MemberAccessOperationSyntax, PrimaryExpressionSyntax,
        StructConstructionBodySyntax, StructFieldInitializerSyntax, SyntaxKind, SyntaxText,
        TupleExpressionSyntax,
    };

    #[test]
    fn expressions_store_operator_children_and_exact_text() {
        let snapshot = test_snapshot("syntax-expression-test", "left + right");

        let mut left = PrimaryExpressionSyntax::builder(snapshot.clone(), TextSize::new(0));

        left.push_token(keyword(SyntaxKind::IdentifierToken, 0, 4, true));

        let mut left_expression = ExpressionSyntax::builder(snapshot.clone(), TextSize::new(0));

        left_expression.push_primary_expression(left.build());

        let mut right = PrimaryExpressionSyntax::builder(snapshot.clone(), TextSize::new(7));

        right.push_token(token(SyntaxKind::IdentifierToken, 7, 12));

        let mut right_expression = ExpressionSyntax::builder(snapshot.clone(), TextSize::new(7));

        right_expression.push_primary_expression(right.build());

        let mut builder = ExpressionSyntax::builder(snapshot, TextSize::new(0));

        builder.push_expression(left_expression.build());
        builder.push_operator_token(keyword(SyntaxKind::PlusToken, 5, 6, true));
        builder.push_expression(right_expression.build());

        let expression = builder.build();
        let children = expression.expressions().collect::<Vec<_>>();

        assert_eq!(expression.full_text(), "left + right");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::PlusToken)
        );

        assert_eq!(children.len(), 2);
    }

    #[test]
    fn primary_expressions_store_typed_roots_and_postfixes() {
        let snapshot = test_snapshot("syntax-primary-expression-test", "target.call(1)");
        let expression_source = snapshot.clone();

        let mut access = AccessExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut member = MemberAccessOperationSyntax::builder(snapshot.clone(), TextSize::new(6));
        let mut list = ArgumentListSyntax::builder(snapshot.clone(), TextSize::new(11));
        let mut argument = ArgumentSyntax::builder(snapshot.clone(), TextSize::new(12));
        let mut primary = PrimaryExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut call = crate::CallOperationSyntax::builder(snapshot.clone(), TextSize::new(11));
        let mut expression = ExpressionSyntax::builder(snapshot, TextSize::ZERO);

        access.push_identifier_token(token(SyntaxKind::IdentifierToken, 0, 6));

        member.push_dot_token(token(SyntaxKind::DotToken, 6, 7));
        member.push_identifier_token(token(SyntaxKind::IdentifierToken, 7, 11));
        access.push_member_access_operation(member.build());

        argument.push_expression(token_expression(
            expression_source,
            SyntaxKind::DecimalIntegerLiteralToken,
            12,
            13,
        ));

        list.push_open_paren_token(token(SyntaxKind::OpenParenToken, 11, 12));
        list.push_argument(argument.build());
        list.push_close_paren_token(token(SyntaxKind::CloseParenToken, 13, 14));

        primary.push_access_expression(access.build());
        call.push_argument_list(list.build());

        expression.push_primary_expression(primary.build());
        expression.push_call_operation(call.build());

        let expression = expression.build();

        assert_eq!(expression.full_text(), "target.call(1)");
        assert_eq!(expression.call_operations().count(), 1);
    }

    #[test]
    fn construction_tuples_and_literals_store_typed_children() {
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

        let tuple_snapshot = test_snapshot("syntax-tuple-expression-test", "(true,)");

        let mut literal =
            LiteralExpressionSyntax::builder(tuple_snapshot.clone(), TextSize::new(1));

        let mut tuple = TupleExpressionSyntax::builder(tuple_snapshot.clone(), TextSize::ZERO);

        literal.push_literal_token(token(SyntaxKind::TrueKeyword, 1, 5));

        let mut literal_primary =
            PrimaryExpressionSyntax::builder(tuple_snapshot.clone(), TextSize::new(1));

        literal_primary.push_literal_expression(literal.build());

        let mut literal_expression = ExpressionSyntax::builder(tuple_snapshot, TextSize::new(1));

        literal_expression.push_primary_expression(literal_primary.build());

        tuple.push_open_paren_token(token(SyntaxKind::OpenParenToken, 0, 1));
        tuple.push_expression(literal_expression.build());
        tuple.push_separator_token(token(SyntaxKind::CommaToken, 5, 6));
        tuple.push_close_paren_token(token(SyntaxKind::CloseParenToken, 6, 7));

        assert_eq!(body.build().full_text(), "{ x = 1}");
        assert_eq!(tuple.build().full_text(), "(true,)");
    }
}
