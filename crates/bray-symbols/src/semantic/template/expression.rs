use bray_declarations::SyntaxAnchor;

use crate::AnySymbolId;

/// One declaration-owned expression retained for later semantic checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclarationExpressionTemplate {
    owner: AnySymbolId,
    syntax: SyntaxAnchor,
}

impl DeclarationExpressionTemplate {
    /// Creates a template for an exact source expression occurrence.
    pub const fn new(owner: AnySymbolId, syntax: SyntaxAnchor) -> Self {
        Self { owner, syntax }
    }

    /// Returns the declaration whose lexical context owns the expression.
    pub const fn owner(self) -> AnySymbolId {
        self.owner
    }

    /// Returns the exact expression syntax occurrence.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }
}

/// An optional declaration default retained without checking its expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UnevaluatedDefaultTemplate {
    /// The declaration has no default expression.
    Absent,
    /// The declaration has this exact source default expression.
    Present(DeclarationExpressionTemplate),
    /// A source-independent imported default is available through its checked provider.
    Resolved,
}

impl UnevaluatedDefaultTemplate {
    /// Returns the source expression when a default was declared.
    pub const fn expression(self) -> Option<DeclarationExpressionTemplate> {
        match self {
            Self::Absent => None,
            Self::Present(expression) => Some(expression),
            Self::Resolved => None,
        }
    }

    /// Returns whether this declaration has a default template.
    pub const fn is_present(self) -> bool {
        matches!(self, Self::Present(_) | Self::Resolved)
    }
}
