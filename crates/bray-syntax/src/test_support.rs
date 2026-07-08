use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
};

use crate::{
    BlockExpressionSyntax, CallableBodyBlockExpressionSyntax, DirectiveArgumentSyntax,
    ExpressionSyntax, ImplementationBodySyntax, ImplementationSubjectSyntax,
    ImplementationTypeMemberBindingSyntax, PathSyntax, PrimaryExpressionSyntax, SyntaxKind,
    SyntaxToken, SyntaxTrivia, TraitApplicationSyntax, TypeAnnotationSyntax, TypeExpressionSyntax,
    TypedIdentifierSyntax,
};

pub(crate) fn func_keyword_with_trailing_space() -> SyntaxToken {
    keyword(SyntaxKind::FuncKeyword, 0, 4, true)
}

pub(crate) fn func_keyword() -> SyntaxToken {
    token(SyntaxKind::FuncKeyword, 0, 4)
}

pub(crate) fn keyword(
    kind: SyntaxKind,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> SyntaxToken {
    let token = token(kind, start, end);

    if has_trailing_space {
        return token.with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(end),
            TextSize::new(end + 1),
        ))]);
    }

    token
}

pub(crate) fn token(kind: SyntaxKind, start: u32, end: u32) -> SyntaxToken {
    SyntaxToken::new(
        kind,
        TextRange::new(TextSize::new(start), TextSize::new(end)),
    )
}

pub(crate) fn identifier_path(snapshot: SourceSnapshot, start: u32, end: u32) -> PathSyntax {
    identifier_path_with_optional_trailing_space(snapshot, start, end, false)
}

pub(crate) fn identifier_path_with_trailing_space(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
) -> PathSyntax {
    identifier_path_with_optional_trailing_space(snapshot, start, end, true)
}

pub(crate) fn identifier_type_expression(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
) -> TypeExpressionSyntax {
    identifier_type_expression_with_optional_trailing_space(snapshot, start, end, false)
}

pub(crate) fn identifier_type_expression_with_trailing_space(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
) -> TypeExpressionSyntax {
    identifier_type_expression_with_optional_trailing_space(snapshot, start, end, true)
}

pub(crate) fn typed_identifier(
    snapshot: SourceSnapshot,
    identifier_start: u32,
    identifier_end: u32,
    type_start: u32,
    type_end: u32,
    type_has_trailing_space: bool,
) -> TypedIdentifierSyntax {
    let mut builder =
        TypedIdentifierSyntax::builder(snapshot.clone(), TextSize::new(identifier_start));

    builder.push_identifier_token(token(
        SyntaxKind::IdentifierToken,
        identifier_start,
        identifier_end,
    ));

    builder.push_type_annotation(type_annotation(
        snapshot,
        identifier_end,
        identifier_end + 1,
        type_start,
        type_end,
        type_has_trailing_space,
    ));

    builder.build()
}

pub(crate) fn directive_argument(
    snapshot: SourceSnapshot,
    kind: SyntaxKind,
    start: u32,
    end: u32,
) -> DirectiveArgumentSyntax {
    let mut builder = DirectiveArgumentSyntax::builder(snapshot.clone(), TextSize::new(start));

    builder.push_expression(token_expression(snapshot, kind, start, end));

    builder.build()
}

pub(crate) fn token_expression(
    snapshot: SourceSnapshot,
    kind: SyntaxKind,
    start: u32,
    end: u32,
) -> ExpressionSyntax {
    token_expression_from_token(snapshot, token(kind, start, end))
}

pub(crate) fn token_expression_from_token(
    snapshot: SourceSnapshot,
    token: SyntaxToken,
) -> ExpressionSyntax {
    let start = token.full_range().start();
    let mut primary = PrimaryExpressionSyntax::builder(snapshot.clone(), start);

    primary.push_token(token);

    let mut expression = ExpressionSyntax::builder(snapshot, start);

    expression.push_primary_expression(primary.build());

    expression.build()
}

pub(crate) fn implementation_subject(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> ImplementationSubjectSyntax {
    let mut builder = ImplementationSubjectSyntax::builder(snapshot.clone(), TextSize::new(start));

    if has_trailing_space {
        builder.push_path(identifier_path_with_trailing_space(snapshot, start, end));
    } else {
        builder.push_path(identifier_path(snapshot, start, end));
    }

    builder.build()
}

pub(crate) fn trait_application(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
) -> TraitApplicationSyntax {
    let mut builder = TraitApplicationSyntax::builder(snapshot.clone(), TextSize::new(start));

    builder.push_path(identifier_path(snapshot, start, end));

    builder.build()
}

pub(crate) fn implementation_body(
    snapshot: SourceSnapshot,
    start: u32,
    has_trailing_space: bool,
) -> ImplementationBodySyntax {
    let mut builder = ImplementationBodySyntax::builder(snapshot, TextSize::new(start));

    builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
    builder.push_close_brace_token(close_brace_token(start, has_trailing_space));

    builder.build()
}

pub(crate) fn block_expression(
    snapshot: SourceSnapshot,
    start: u32,
    has_trailing_space: bool,
) -> BlockExpressionSyntax {
    let mut builder = BlockExpressionSyntax::builder(snapshot, TextSize::new(start));

    builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
    builder.push_close_brace_token(close_brace_token(start, has_trailing_space));

    builder.build()
}

pub(crate) fn callable_body_block_expression(
    snapshot: SourceSnapshot,
    start: u32,
    has_trailing_space: bool,
) -> CallableBodyBlockExpressionSyntax {
    let mut builder =
        CallableBodyBlockExpressionSyntax::builder(snapshot.clone(), TextSize::new(start));

    builder.push_block_expression(block_expression(snapshot, start, has_trailing_space));

    builder.build()
}

pub(crate) fn implementation_type_member_binding(
    snapshot: SourceSnapshot,
    start: u32,
    has_trailing_space: bool,
) -> ImplementationTypeMemberBindingSyntax {
    let mut builder =
        ImplementationTypeMemberBindingSyntax::builder(snapshot.clone(), TextSize::new(start));

    builder.push_type_keyword(keyword(SyntaxKind::TypeKeyword, start, start + 4, true));
    builder.push_identifier_token(keyword(
        SyntaxKind::IdentifierToken,
        start + 5,
        start + 9,
        true,
    ));
    builder.push_equals_token(keyword(
        SyntaxKind::EqualsToken,
        start + 10,
        start + 11,
        true,
    ));
    builder.push_type_expression(identifier_type_expression(snapshot, start + 12, start + 19));
    builder.push_semicolon_token(keyword(
        SyntaxKind::SemicolonToken,
        start + 19,
        start + 20,
        has_trailing_space,
    ));

    builder.build()
}

fn identifier_type_expression_with_optional_trailing_space(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> TypeExpressionSyntax {
    let mut builder = TypeExpressionSyntax::builder(snapshot.clone(), TextSize::new(start));

    if has_trailing_space {
        builder.push_path(identifier_path_with_trailing_space(snapshot, start, end));
    } else {
        builder.push_path(identifier_path(snapshot, start, end));
    }

    builder.build()
}

fn type_annotation(
    snapshot: SourceSnapshot,
    colon_start: u32,
    colon_end: u32,
    type_start: u32,
    type_end: u32,
    type_has_trailing_space: bool,
) -> TypeAnnotationSyntax {
    let mut builder = TypeAnnotationSyntax::builder(snapshot.clone(), TextSize::new(colon_start));

    builder.push_colon_token(keyword(
        SyntaxKind::ColonToken,
        colon_start,
        colon_end,
        true,
    ));

    if type_has_trailing_space {
        builder.push_type_expression(identifier_type_expression_with_trailing_space(
            snapshot, type_start, type_end,
        ));
    } else {
        builder.push_type_expression(identifier_type_expression(snapshot, type_start, type_end));
    }

    builder.build()
}

fn identifier_path_with_optional_trailing_space(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> PathSyntax {
    let mut builder = PathSyntax::builder(snapshot);

    builder.push_identifier_token(keyword(
        SyntaxKind::IdentifierToken,
        start,
        end,
        has_trailing_space,
    ));

    builder.build()
}

fn close_brace_token(start: u32, has_trailing_space: bool) -> SyntaxToken {
    if has_trailing_space {
        return keyword(SyntaxKind::CloseBraceToken, start + 1, start + 2, true);
    }

    token(SyntaxKind::CloseBraceToken, start + 1, start + 2)
}

pub(crate) fn snapshot(origin_name: &'static str, text: &str) -> SourceSnapshot {
    match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source(origin_name),
        SourceVersion::new(0),
        text,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source should fit in TextSize: {error:?}"),
    }
}
