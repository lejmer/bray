use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::{ModulePartId, SyntaxAnchor};
use bray_syntax::SyntaxKind;

use crate::{AnySymbolId, SymbolName};

use super::DeclarationExpressionTemplate;

/// One language-defined directive category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirectiveKind {
    /// Restricts a module contribution to selected targets.
    Target,
    /// Marks a declaration or module contribution as test-only.
    Test,
    /// Supplies native link requirements.
    Link,
    /// Selects a type layout policy.
    Layout,
    /// Marks a type as implicitly copyable.
    Copy,
    /// Supplies an explicit union variant tag.
    Tag,
    /// Selects a callable application binary interface.
    Abi,
    /// Supplies an external symbol spelling.
    Symbol,
    /// Marks a function as an executable entry point.
    Entrypoint,
    /// Gives a static declaration one instance per exact native-thread attachment.
    ThreadLocal,
}

impl DirectiveKind {
    /// Maps one concrete directive syntax kind to its semantic category.
    pub const fn try_from_syntax_kind(kind: SyntaxKind) -> Option<Self> {
        match kind {
            SyntaxKind::TargetDirective => Some(Self::Target),
            SyntaxKind::TestDirective => Some(Self::Test),
            SyntaxKind::LinkDirective => Some(Self::Link),
            SyntaxKind::LayoutDirective => Some(Self::Layout),
            SyntaxKind::CopyDirective => Some(Self::Copy),
            SyntaxKind::TagDirective => Some(Self::Tag),
            SyntaxKind::AbiDirective => Some(Self::Abi),
            SyntaxKind::SymbolDirective => Some(Self::Symbol),
            SyntaxKind::EntrypointDirective => Some(Self::Entrypoint),
            SyntaxKind::ThreadLocalDirective => Some(Self::ThreadLocal),
            _ => None,
        }
    }
}

/// The source attachment that owns one directive occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirectiveAttachment {
    /// A declaration symbol owns the directive.
    Declaration(AnySymbolId),
    /// One source contribution to a partial module owns the directive.
    ModulePart(ModulePartId),
    /// One callable type expression owns the directive.
    CallableType(SyntaxAnchor),
}

/// Stable identity of one callable type's directive surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableTypeDirectiveKey {
    owner: AnySymbolId,
    syntax: SyntaxAnchor,
}

impl CallableTypeDirectiveKey {
    /// Creates a key from the declaration context and exact callable type syntax.
    pub const fn new(owner: AnySymbolId, syntax: SyntaxAnchor) -> Self {
        Self { owner, syntax }
    }

    /// Returns the declaration whose lexical context contains the callable type.
    pub const fn owner(self) -> AnySymbolId {
        self.owner
    }

    /// Returns the exact callable type syntax.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }
}

/// The source form of one directive argument name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DirectiveArgumentName {
    /// The argument was positional.
    Positional,
    /// The argument used a valid explicit name.
    Named(SymbolName),
    /// Parser recovery prevented a valid argument name from being retained.
    Recovered,
}

/// One directive argument retained for domain-specific semantic checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectiveArgumentTemplate {
    name: DirectiveArgumentName,
    expression: DeclarationExpressionTemplate,
}

impl DirectiveArgumentTemplate {
    /// Creates one source-backed directive argument.
    pub const fn new(
        name: DirectiveArgumentName,
        expression: DeclarationExpressionTemplate,
    ) -> Self {
        Self { name, expression }
    }

    /// Returns the source form of the argument name.
    pub const fn name(&self) -> &DirectiveArgumentName {
        &self.name
    }

    /// Returns the unevaluated argument expression.
    pub const fn expression(&self) -> DeclarationExpressionTemplate {
        self.expression
    }
}

/// One source-backed directive occurrence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectiveTemplate {
    kind: DirectiveKind,
    syntax: SyntaxAnchor,
    attachment: DirectiveAttachment,
    arguments: Arc<[DirectiveArgumentTemplate]>,
}

impl DirectiveTemplate {
    /// Creates one directive while preserving its source argument order.
    pub fn new(
        kind: DirectiveKind,
        syntax: SyntaxAnchor,
        attachment: DirectiveAttachment,
        arguments: impl IntoIterator<Item = DirectiveArgumentTemplate>,
    ) -> Self {
        Self {
            kind,
            syntax,
            attachment,
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the language-defined directive category.
    pub const fn kind(&self) -> DirectiveKind {
        self.kind
    }

    /// Returns the exact directive syntax.
    pub const fn syntax(&self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns the source attachment that owns this occurrence.
    pub const fn attachment(&self) -> DirectiveAttachment {
        self.attachment
    }

    /// Returns arguments in source order.
    pub fn arguments(&self) -> &[DirectiveArgumentTemplate] {
        &self.arguments
    }
}

/// Ordered directive occurrences attached to one semantic surface.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectiveSurface {
    directives: Arc<[DirectiveTemplate]>,
}

impl DirectiveSurface {
    /// Creates a directive surface without sorting or deduplicating occurrences.
    pub fn new(directives: impl IntoIterator<Item = DirectiveTemplate>) -> Self {
        Self {
            directives: shared_slice(directives),
        }
    }

    /// Returns directive occurrences in source order.
    pub fn directives(&self) -> &[DirectiveTemplate] {
        &self.directives
    }

    /// Returns whether the surface contains no directive occurrences.
    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::DirectiveSurface;

    #[test]
    fn directive_surfaces_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DirectiveSurface>();
    }
}
