use bray_syntax::SyntaxKind;

use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a declaration-name argument.
    pub fn declaration_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::DeclarationName,
            DiagnosticArgValue::DeclarationName(name.into()),
        )
    }

    /// Creates a named trait-member argument.
    pub fn trait_member_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::TraitMemberName,
            DiagnosticArgValue::DeclarationName(name.into()),
        )
    }

    /// Creates a fixed-keyword trait-member argument.
    pub const fn trait_member_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::TraitMemberName,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a referenced-name argument.
    pub fn referenced_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ReferencedName,
            DiagnosticArgValue::ReferencedName(name.into()),
        )
    }

    /// Creates an expected semantic-name category argument.
    pub const fn expected_name_kind(kind: DiagnosticNameKind) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedNameKind,
            DiagnosticArgValue::NameKind(kind),
        )
    }
}

/// Locale-neutral semantic category expected from name binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNameKind {
    /// Any ordinary declaration or local name.
    Symbol,
    /// A logical module.
    Module,
    /// A type-valued declaration or parameter.
    Type,
    /// A trait declaration.
    Trait,
    /// A runtime or compile-time value.
    Value,
    /// A declaration usable as a named pattern.
    Pattern,
    /// An explicit callable overload family.
    CallableOverload,
    /// A declaration associated with a module, type, trait, or implementation.
    Member,
    /// A compiler-known capability allowed in a trusted callable contract.
    TrustedCapability,
}

impl DiagnosticNameKind {
    /// Returns the stable machine key for this category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Module => "module",
            Self::Type => "type",
            Self::Trait => "trait",
            Self::Value => "value",
            Self::Pattern => "pattern",
            Self::CallableOverload => "callable_overload",
            Self::Member => "member",
            Self::TrustedCapability => "trusted_capability",
        }
    }
}
