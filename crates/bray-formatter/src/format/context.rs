use bray_syntax::SyntaxKind;

use crate::FormatterRule;

#[derive(Clone, Copy)]
pub(super) struct TokenSpacing {
    rule: FormatterRule,
    uses_space: bool,
}

impl TokenSpacing {
    pub(super) const fn rule(self) -> FormatterRule {
        self.rule
    }

    pub(super) const fn uses_space(self) -> bool {
        self.uses_space
    }
}

pub(super) fn token_spacing(
    previous: Option<SyntaxKind>,
    previous_operator_was_prefix: bool,
    previous_was_generic_delimiter: bool,
    current: SyntaxKind,
    current_operator_is_prefix: bool,
    current_is_generic_delimiter: bool,
) -> Option<TokenSpacing> {
    let previous = previous?;

    if current_is_generic_delimiter {
        return Some(no_space(FormatterRule::GenericDelimiterSpacing));
    }

    if previous_was_generic_delimiter {
        return Some(TokenSpacing {
            rule: FormatterRule::GenericDelimiterSpacing,
            uses_space: is_word(current) || is_operator(current),
        });
    }

    if previous == SyntaxKind::DotDotToken
        && matches!(current, SyntaxKind::PlusToken | SyntaxKind::MinusToken)
    {
        return Some(TokenSpacing {
            rule: FormatterRule::RangeSpacing,
            uses_space: true,
        });
    }

    if previous == SyntaxKind::DotDotToken || current == SyntaxKind::DotDotToken {
        return Some(no_space(FormatterRule::RangeSpacing));
    }

    if matches!(previous, SyntaxKind::DotToken) || matches!(current, SyntaxKind::DotToken) {
        return Some(no_space(FormatterRule::MemberAccessSpacing));
    }

    if matches!(previous, SyntaxKind::AtToken) {
        return Some(no_space(FormatterRule::DirectiveMarkerSpacing));
    }

    if is_operator(previous) {
        return Some(TokenSpacing {
            rule: if previous_operator_was_prefix {
                FormatterRule::PrefixOperatorSpacing
            } else {
                FormatterRule::OperatorSpacing
            },
            uses_space: !previous_operator_was_prefix,
        });
    }

    if let Some(spacing) = punctuation_spacing(previous, current) {
        return Some(spacing);
    }

    if is_operator(current) {
        let uses_space = !current_operator_is_prefix
            || is_word(previous)
            || (is_operator(previous) && !previous_operator_was_prefix);

        return Some(TokenSpacing {
            rule: if current_operator_is_prefix && !uses_space {
                FormatterRule::PrefixOperatorSpacing
            } else {
                FormatterRule::OperatorSpacing
            },
            uses_space,
        });
    }

    (is_word(previous) || closes_delimiter(previous)).then_some(TokenSpacing {
        rule: FormatterRule::WordSpacing,
        uses_space: true,
    })
}

pub(super) fn is_operator(kind: SyntaxKind) -> bool {
    (kind.is_expression_operator() && !matches!(kind, SyntaxKind::AtToken | SyntaxKind::ColonToken))
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

pub(super) fn is_generic_delimiter(kind: SyntaxKind, parent: Option<SyntaxKind>) -> bool {
    matches!(kind, SyntaxKind::LessToken | SyntaxKind::GreaterToken) && generic_list(parent)
}

pub(super) const fn comma_layout_rule(parent: Option<SyntaxKind>) -> Option<FormatterRule> {
    match parent {
        Some(SyntaxKind::StructConstructionBody) => Some(FormatterRule::StructConstructionLayout),
        Some(SyntaxKind::OverloadArmList) => Some(FormatterRule::OverloadArmLayout),
        _ => None,
    }
}

pub(super) const fn list_layout_rule(parent: Option<SyntaxKind>) -> Option<FormatterRule> {
    match parent {
        Some(
            SyntaxKind::DirectiveArgumentList
            | SyntaxKind::RequiresClause
            | SyntaxKind::EnsuresClause
            | SyntaxKind::WithClause
            | SyntaxKind::UsesClause
            | SyntaxKind::UnionVariantPayload
            | SyntaxKind::PredicateParameterList
            | SyntaxKind::ParameterList
            | SyntaxKind::ArgumentList
            | SyntaxKind::AssertionExpression
            | SyntaxKind::TypeFormConstructionExpression
            | SyntaxKind::BooleanFoldExpression
            | SyntaxKind::TupleExpression,
        ) => Some(FormatterRule::ParenthesizedListLayout),
        Some(
            SyntaxKind::ArrayExpression
            | SyntaxKind::ElementIndexOperation
            | SyntaxKind::SliceIndexOperation,
        ) => Some(FormatterRule::BracketedListLayout),
        Some(
            SyntaxKind::GenericParameterList
            | SyntaxKind::GenericArgumentList
            | SyntaxKind::TypeFormArgumentList,
        ) => Some(FormatterRule::GenericListLayout),
        _ => None,
    }
}

pub(super) const fn is_list_delimiter(kind: SyntaxKind, parent: Option<SyntaxKind>) -> bool {
    match list_layout_rule(parent) {
        Some(FormatterRule::ParenthesizedListLayout) => {
            matches!(
                kind,
                SyntaxKind::OpenParenToken | SyntaxKind::CloseParenToken
            )
        }
        Some(FormatterRule::BracketedListLayout) => {
            matches!(
                kind,
                SyntaxKind::OpenBracketToken | SyntaxKind::CloseBracketToken
            )
        }
        Some(FormatterRule::GenericListLayout) => {
            matches!(kind, SyntaxKind::LessToken | SyntaxKind::GreaterToken)
        }
        _ => false,
    }
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

pub(super) fn is_callable_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FunctionDeclaration
            | SyntaxKind::TraitCallableMemberDeclaration
            | SyntaxKind::TypeCallableMemberDeclaration
            | SyntaxKind::TypeConstructorMemberDeclaration
            | SyntaxKind::FinalizerMemberDeclaration
            | SyntaxKind::DestructorMemberDeclaration
            | SyntaxKind::ScopeEnterMemberDeclaration
            | SyntaxKind::ScopeExitMemberDeclaration
            | SyntaxKind::TraitFinalizerRequirementDeclaration
            | SyntaxKind::TraitDestructorRequirementDeclaration
            | SyntaxKind::TraitScopeEnterRequirementDeclaration
            | SyntaxKind::TraitScopeExitRequirementDeclaration
    )
}

pub(super) fn separation_rule(
    kind: SyntaxKind,
    parent: Option<SyntaxKind>,
) -> Option<FormatterRule> {
    if matches!(
        parent,
        Some(SyntaxKind::SourceUnit | SyntaxKind::ModuleBody)
    ) {
        return is_module_declaration(kind).then_some(FormatterRule::ModuleItemSpacing);
    }

    is_callable_member(kind).then_some(FormatterRule::CallableMemberSpacing)
}

pub(super) fn leading_separation_rule(
    kind: SyntaxKind,
    parent: Option<SyntaxKind>,
    previous: Option<SyntaxKind>,
) -> Option<FormatterRule> {
    if matches!(
        parent,
        Some(SyntaxKind::SourceUnit | SyntaxKind::ModuleBody)
    ) && is_ordinary_module_declaration(kind)
        && previous == Some(SyntaxKind::SemicolonToken)
    {
        return Some(FormatterRule::ModuleItemSpacing);
    }

    is_callable_member(kind)
        .then_some(FormatterRule::CallableMemberSpacing)
        .filter(|_| previous == Some(SyntaxKind::SemicolonToken))
}

pub(super) fn is_line_comment(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LineCommentTrivia | SyntaxKind::DocumentationLineCommentTrivia
    )
}

fn punctuation_spacing(previous: SyntaxKind, current: SyntaxKind) -> Option<TokenSpacing> {
    match previous {
        SyntaxKind::CommaToken => return Some(space(FormatterRule::CommaSpacing)),
        SyntaxKind::SemicolonToken => return Some(space(FormatterRule::SemicolonLayout)),
        SyntaxKind::ColonToken => return Some(space(FormatterRule::ColonSpacing)),
        _ => {}
    }

    match current {
        SyntaxKind::OpenParenToken | SyntaxKind::CloseParenToken => {
            Some(no_space(FormatterRule::ParenthesizedListLayout))
        }
        SyntaxKind::OpenBracketToken if previous == SyntaxKind::MutKeyword => {
            Some(space(FormatterRule::WordSpacing))
        }
        SyntaxKind::OpenBracketToken | SyntaxKind::CloseBracketToken => {
            Some(no_space(FormatterRule::BracketedListLayout))
        }
        SyntaxKind::CommaToken => Some(no_space(FormatterRule::CommaSpacing)),
        SyntaxKind::SemicolonToken => Some(no_space(FormatterRule::SemicolonLayout)),
        SyntaxKind::ColonToken => Some(no_space(FormatterRule::ColonSpacing)),
        SyntaxKind::QuestionToken if previous == SyntaxKind::CaseKeyword => {
            Some(space(FormatterRule::WordSpacing))
        }
        SyntaxKind::QuestionToken => Some(no_space(FormatterRule::MemberAccessSpacing)),
        _ => match previous {
            SyntaxKind::OpenParenToken => Some(no_space(FormatterRule::ParenthesizedListLayout)),
            SyntaxKind::OpenBracketToken => Some(no_space(FormatterRule::BracketedListLayout)),
            _ => None,
        },
    }
}

const fn no_space(rule: FormatterRule) -> TokenSpacing {
    TokenSpacing {
        rule,
        uses_space: false,
    }
}

const fn space(rule: FormatterRule) -> TokenSpacing {
    TokenSpacing {
        rule,
        uses_space: true,
    }
}

const fn generic_list(parent: Option<SyntaxKind>) -> bool {
    matches!(
        parent,
        Some(
            SyntaxKind::GenericParameterList
                | SyntaxKind::GenericArgumentList
                | SyntaxKind::TypeFormArgumentList
        )
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
            | SyntaxKind::ConstantDeclaration
            | SyntaxKind::StaticDeclaration
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

fn is_ordinary_module_declaration(kind: SyntaxKind) -> bool {
    is_module_declaration(kind)
        && !matches!(
            kind,
            SyntaxKind::SourceUnitModuleDeclaration | SyntaxKind::BlockModuleDeclaration
        )
}

fn is_callable_member(kind: SyntaxKind) -> bool {
    is_callable_declaration(kind) && kind != SyntaxKind::FunctionDeclaration
}
