use bray_bound_tree::{
    BoundPatternKind, BoundPatternLiteral, BoundPatternMode, BoundPatternTarget,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::SymbolName;
use bray_syntax::{CasePatternSyntax, IrrefutablePatternSyntax, SourceSyntaxNode, SyntaxToken};

use crate::binder::PatternBindingMode;
use crate::binding::expression::literal_kind;
use crate::binding::name::symbol_name;

pub(super) const fn bound_mode(mode: PatternBindingMode) -> BoundPatternMode {
    match mode {
        PatternBindingMode::Declaration => BoundPatternMode::Declaration,
        PatternBindingMode::Assignment => BoundPatternMode::Assignment,
        PatternBindingMode::MatchObserve => BoundPatternMode::MatchObserve,
        PatternBindingMode::MatchConsume => BoundPatternMode::MatchConsume,
    }
}

pub(super) fn pattern_kind(
    syntax: &impl PatternSyntax,
    is_binding: bool,
    target: Option<BoundPatternTarget>,
    has_alternatives: bool,
) -> BoundPatternKind {
    if has_alternatives && syntax.has_alternative_separator() {
        BoundPatternKind::Alternative
    } else if is_binding {
        BoundPatternKind::Binding
    } else if matches!(
        target,
        Some(BoundPatternTarget::Surface(
            bray_symbols::AnySymbolId::UnionVariant(_)
        ))
    ) {
        BoundPatternKind::Variant
    } else if target.is_some() {
        BoundPatternKind::Path
    } else if syntax.has_discard() {
        BoundPatternKind::Discard
    } else if syntax.has_none() {
        BoundPatternKind::NullableAbsent
    } else if syntax.has_question() {
        BoundPatternKind::NullablePresent
    } else if syntax.has_box() {
        BoundPatternKind::Box
    } else if syntax.has_dot() || syntax.has_path() && syntax.has_open_paren() {
        BoundPatternKind::Variant
    } else if syntax.has_open_brace() {
        BoundPatternKind::Product
    } else if syntax.has_open_bracket() {
        BoundPatternKind::Array
    } else if syntax.has_open_paren() && syntax.has_comma() {
        BoundPatternKind::Tuple
    } else if syntax.has_open_paren() {
        BoundPatternKind::Grouped
    } else if syntax.has_literal() {
        BoundPatternKind::Literal
    } else if syntax.has_path() {
        BoundPatternKind::Path
    } else if syntax.has_dot_dot() {
        BoundPatternKind::Remaining
    } else {
        BoundPatternKind::Error
    }
}

// This private adapter unifies binder operations over existing typed syntax nodes.
// It does not define syntax tree structure, so it belongs here rather than in bray-syntax.
pub(super) trait PatternSyntax: SourceSyntaxNode {
    fn alternative_binding_occurrences(&self) -> Vec<Vec<BindingOccurrence>>;
    fn simple_binding_token(&self) -> Option<SyntaxToken>;
    fn bare_name_token(&self) -> Option<SyntaxToken>;
    fn first_path(&self) -> Option<bray_syntax::PathSyntax>;
    fn pattern_name(&self) -> Option<SymbolName>;
    fn pattern_literal(&self) -> Option<BoundPatternLiteral>;
    fn has_alternative_separator(&self) -> bool;
    fn has_discard(&self) -> bool;
    fn has_literal(&self) -> bool;
    fn has_none(&self) -> bool;
    fn has_question(&self) -> bool;
    fn has_box(&self) -> bool;
    fn has_dot(&self) -> bool;
    fn has_open_brace(&self) -> bool;
    fn has_open_bracket(&self) -> bool;
    fn has_open_paren(&self) -> bool;
    fn has_comma(&self) -> bool;
    fn has_path(&self) -> bool;
    fn has_dot_dot(&self) -> bool;
}

macro_rules! impl_pattern_syntax {
    ($syntax:ty, $alternatives:expr, $occurrences:expr) => {
        impl PatternSyntax for $syntax {
            fn alternative_binding_occurrences(&self) -> Vec<Vec<BindingOccurrence>> {
                ($occurrences)(self)
            }

            fn simple_binding_token(&self) -> Option<SyntaxToken> {
                let token = self.bare_name_token()?;

                (self.open_paren_token().is_none()
                    && self.open_bracket_token().is_none()
                    && self.open_brace_token().is_none())
                .then_some(token)
            }

            fn bare_name_token(&self) -> Option<SyntaxToken> {
                let path_token = self.paths().next().and_then(|path| {
                    let mut tokens = path.identifier_tokens();
                    let first = tokens.next()?;

                    tokens.next().is_none().then_some(first)
                });

                self.dot_token()
                    .is_none()
                    .then(|| self.identifier_token().or(path_token))
                    .flatten()
            }

            fn first_path(&self) -> Option<bray_syntax::PathSyntax> {
                self.paths().next()
            }

            fn pattern_name(&self) -> Option<SymbolName> {
                self.identifier_token()
                    .and_then(|token| symbol_name(self.source(), &token))
                    .or_else(|| {
                        self.paths()
                            .next()
                            .and_then(|path| path.identifier_tokens().last())
                            .and_then(|token| symbol_name(self.source(), &token))
                    })
            }

            fn pattern_literal(&self) -> Option<BoundPatternLiteral> {
                let token = self.literal_token()?;
                let kind = literal_kind(token.kind())?;

                Some(BoundPatternLiteral::new(kind, token.range()))
            }

            fn has_alternative_separator(&self) -> bool {
                ($alternatives)(self)
            }

            fn has_discard(&self) -> bool {
                self.discard_token().is_some()
            }

            fn has_literal(&self) -> bool {
                self.literal_token().is_some()
            }

            fn has_none(&self) -> bool {
                self.none_keyword().is_some()
            }

            fn has_question(&self) -> bool {
                self.question_token().is_some()
            }

            fn has_box(&self) -> bool {
                self.box_keyword().is_some()
            }

            fn has_dot(&self) -> bool {
                self.dot_token().is_some()
            }

            fn has_open_brace(&self) -> bool {
                self.open_brace_token().is_some()
            }

            fn has_open_bracket(&self) -> bool {
                self.open_bracket_token().is_some()
            }

            fn has_open_paren(&self) -> bool {
                self.open_paren_token().is_some()
            }

            fn has_comma(&self) -> bool {
                self.comma_token().is_some()
            }

            fn has_path(&self) -> bool {
                self.paths().next().is_some()
            }

            fn has_dot_dot(&self) -> bool {
                self.dot_dot_token().is_some()
            }
        }
    };
}

impl_pattern_syntax!(IrrefutablePatternSyntax, |_| false, |_| Vec::new());
impl_pattern_syntax!(
    CasePatternSyntax,
    |syntax: &CasePatternSyntax| syntax.alternative_separator_tokens().next().is_some(),
    case_alternative_binding_occurrences
);

pub(super) struct BindingOccurrence {
    pub(super) name: SymbolName,
    pub(super) anchor: SyntaxAnchor,
    pub(super) is_recovered: bool,
}

pub(super) fn case_alternative_binding_occurrences(
    syntax: &CasePatternSyntax,
) -> Vec<Vec<BindingOccurrence>> {
    if syntax.alternative_separator_tokens().next().is_none() {
        return Vec::new();
    }

    syntax
        .case_patterns()
        .map(|alternative| {
            let mut occurrences = Vec::new();
            collect_case_binding_occurrences(&alternative, &mut occurrences);

            occurrences
        })
        .collect()
}

fn collect_case_binding_occurrences(
    syntax: &CasePatternSyntax,
    occurrences: &mut Vec<BindingOccurrence>,
) {
    if let Some(token) = syntax.simple_binding_token() {
        push_binding_occurrence(syntax, SyntaxAnchor::from_node(syntax), token, occurrences);
    }

    for entry in syntax.case_pattern_entries() {
        let nested = entry.case_patterns().collect::<Vec<_>>();

        if nested.is_empty()
            && entry.dot_dot_token().is_none()
            && let Some(token) = entry.identifier_token()
        {
            push_binding_occurrence(&entry, SyntaxAnchor::from_node(&entry), token, occurrences);
        }

        for child in nested {
            collect_case_binding_occurrences(&child, occurrences);
        }
    }

    for child in syntax.case_patterns() {
        collect_case_binding_occurrences(&child, occurrences);
    }
}

fn push_binding_occurrence(
    syntax: &impl SourceSyntaxNode,
    anchor: SyntaxAnchor,
    token: SyntaxToken,
    occurrences: &mut Vec<BindingOccurrence>,
) {
    if token.is_missing() {
        return;
    }

    let Some(text) = token.text(syntax.source().text()) else {
        return;
    };

    let Some(name) = SymbolName::try_new(text) else {
        return;
    };

    occurrences.push(BindingOccurrence {
        name,
        anchor,
        is_recovered: anchor.is_recovered(),
    });
}
