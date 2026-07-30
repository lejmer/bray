use bray_syntax::SyntaxKind;

pub(super) fn needs_space_before(
    previous: Option<SyntaxKind>,
    previous_operator_was_prefix: bool,
    previous_was_generic_delimiter: bool,
    current: SyntaxKind,
    current_operator_is_prefix: bool,
    current_is_generic_delimiter: bool,
) -> bool {
    let Some(previous) = previous else {
        return false;
    };

    if no_space_before(current) || no_space_after(previous) {
        return false;
    }

    if previous_was_generic_delimiter || current_is_generic_delimiter {
        return false;
    }

    if matches!(previous, SyntaxKind::DotDotToken)
        || matches!(current, SyntaxKind::DotDotToken)
    {
        return false;
    }

    if is_operator(current) {
        return !current_operator_is_prefix || is_word(previous);
    }

    if is_operator(previous) {
        return !previous_operator_was_prefix;
    }

    is_word(previous) || closes_delimiter(previous)
}

pub(super) fn clears_pending_space_before(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::CommaToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::ColonToken
            | SyntaxKind::DotToken
            | SyntaxKind::QuestionToken
    )
}

pub(super) fn is_operator(kind: SyntaxKind) -> bool {
    (kind.is_expression_operator()
        && !matches!(kind, SyntaxKind::AtToken | SyntaxKind::ColonToken))
        || matches!(kind, SyntaxKind::ArrowToken | SyntaxKind::DotDotToken)
}

pub(super) fn is_prefix_operator(operator: SyntaxKind, previous: SyntaxKind) -> bool {
    if matches!(operator, SyntaxKind::BangToken | SyntaxKind::TildeToken) {
        return true;
    }

    matches!(
        operator,
        SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::StarToken
            | SyntaxKind::AmpersandToken
    ) && !can_end_expression(previous)
}

pub(super) fn is_generic_delimiter(
    kind: SyntaxKind,
    parent: Option<SyntaxKind>,
) -> bool {
    matches!(kind, SyntaxKind::LessToken | SyntaxKind::GreaterToken)
        && matches!(
            parent,
            Some(
                SyntaxKind::GenericParameterList
                    | SyntaxKind::GenericArgumentList
                    | SyntaxKind::TypeFormArgumentList
            )
        )
}

pub(super) fn comma_uses_line_break(parent: Option<SyntaxKind>) -> bool {
    matches!(
        parent,
        Some(SyntaxKind::StructConstructionBody | SyntaxKind::OverloadArmList)
    )
}

pub(super) fn semicolon_stays_inline(parent: Option<SyntaxKind>) -> bool {
    matches!(
        parent,
        Some(SyntaxKind::ArrayExpression | SyntaxKind::TypeExpression)
    )
}

pub(super) fn is_directive(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::TargetDirective
            | SyntaxKind::TestDirective
            | SyntaxKind::LinkDirective
            | SyntaxKind::AbiDirective
            | SyntaxKind::SymbolDirective
            | SyntaxKind::EntrypointDirective
            | SyntaxKind::LayoutDirective
            | SyntaxKind::CopyDirective
            | SyntaxKind::TagDirective
    )
}

pub(super) fn should_separate_after(
    kind: SyntaxKind,
    parent: Option<SyntaxKind>,
) -> bool {
    if matches!(parent, Some(SyntaxKind::SourceUnit | SyntaxKind::ModuleBody)) {
        return is_module_declaration(kind);
    }

    matches!(
        kind,
        SyntaxKind::FunctionDeclaration
            | SyntaxKind::TypeCallableMemberDeclaration
            | SyntaxKind::TypeConstructorMemberDeclaration
            | SyntaxKind::FinalizerMemberDeclaration
            | SyntaxKind::DestructorMemberDeclaration
            | SyntaxKind::ScopeEnterMemberDeclaration
            | SyntaxKind::ScopeExitMemberDeclaration
            | SyntaxKind::TraitCallableMemberDeclaration
    )
}

pub(super) fn is_line_comment(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LineCommentTrivia | SyntaxKind::DocumentationLineCommentTrivia
    )
}

fn no_space_before(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::OpenParenToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::CommaToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::ColonToken
            | SyntaxKind::DotToken
            | SyntaxKind::QuestionToken
    )
}

fn no_space_after(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::DotToken
            | SyntaxKind::AtToken
    )
}

fn is_word(kind: SyntaxKind) -> bool {
    kind.is_keyword()
        || kind.is_literal()
        || matches!(
            kind,
            SyntaxKind::IdentifierToken
                | SyntaxKind::TupleElementIndexToken
                | SyntaxKind::UnderscoreToken
        )
}

fn closes_delimiter(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken | SyntaxKind::CloseBraceToken
    )
}

fn can_end_expression(kind: SyntaxKind) -> bool {
    kind.is_literal()
        || matches!(
            kind,
            SyntaxKind::IdentifierToken
                | SyntaxKind::TupleElementIndexToken
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NoneKeyword
                | SyntaxKind::SelfValueKeyword
                | SyntaxKind::SelfTypeKeyword
                | SyntaxKind::UnitKeyword
        )
        || closes_delimiter(kind)
        || matches!(kind, SyntaxKind::QuestionToken)
}

fn is_module_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::SourceUnitModuleDeclaration
            | SyntaxKind::BlockModuleDeclaration
            | SyntaxKind::UsingDeclaration
            | SyntaxKind::ExportDeclaration
            | SyntaxKind::ConstantDeclaration
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::PredicateDeclaration
            | SyntaxKind::CallableContractDeclaration
            | SyntaxKind::CallableOverloadDeclaration
            | SyntaxKind::ImplementationOverloadDeclaration
            | SyntaxKind::StructDeclaration
            | SyntaxKind::UnionDeclaration
            | SyntaxKind::TraitDeclaration
            | SyntaxKind::InherentImplementationDeclaration
            | SyntaxKind::UnnamedTraitImplementationDeclaration
            | SyntaxKind::NamedTraitImplementationDeclaration
    )
}
