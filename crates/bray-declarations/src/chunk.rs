use bray_source::SourceId;

use crate::name::{DeclarationName, ModulePath};
use crate::record::DeclarationKind;
use crate::surface::{DeclarationSurface, SyntaxAnchor};

/// Immutable declaration discovery output for one source unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DeclarationChunk {
    source_id: SourceId,
    module_parts: Box<[DiscoveredModulePart]>,
}

impl DeclarationChunk {
    pub(crate) fn new(source_id: SourceId, module_parts: Box<[DiscoveredModulePart]>) -> Self {
        Self {
            source_id,
            module_parts,
        }
    }

    /// Returns the source snapshot this chunk was discovered from.
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns module parts discovered in source order.
    pub fn module_parts(&self) -> &[DiscoveredModulePart] {
        &self.module_parts
    }
}

/// Immutable source-unit contribution to a logical module.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiscoveredModulePart {
    pub(crate) path: ModulePath,
    pub(crate) syntax: SyntaxAnchor,
    pub(crate) surface: DeclarationSurface,
    pub(crate) declarations: Box<[DiscoveredDeclaration]>,
}

impl DiscoveredModulePart {
    pub(crate) fn new(
        path: ModulePath,
        syntax: SyntaxAnchor,
        surface: DeclarationSurface,
        declarations: Box<[DiscoveredDeclaration]>,
    ) -> Self {
        Self {
            path,
            syntax,
            surface,
            declarations,
        }
    }

    /// Returns the syntactic module path for this part.
    pub const fn path(&self) -> &ModulePath {
        &self.path
    }

    /// Returns the source snapshot that contains this module part.
    pub const fn source_id(&self) -> SourceId {
        self.syntax.source_id()
    }

    /// Returns the concrete syntax node kind this module part came from.
    pub const fn syntax_kind(&self) -> bray_syntax::SyntaxKind {
        self.syntax.syntax_kind()
    }

    /// Returns the full source range covered by this module declaration syntax.
    pub const fn full_range(&self) -> bray_source::TextRange {
        self.syntax.full_range()
    }

    /// Returns whether this module part syntax contains parser recovery.
    pub const fn is_recovered(&self) -> bool {
        self.syntax.is_recovered()
    }

    /// Returns the stable syntax anchor for this module part.
    pub const fn syntax_anchor(&self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns syntax-backed surface metadata for this module part.
    pub const fn surface(&self) -> &DeclarationSurface {
        &self.surface
    }

    /// Returns module-level declarations contributed by this part in source order.
    pub fn declarations(&self) -> &[DiscoveredDeclaration] {
        &self.declarations
    }
}

/// Immutable source-order declaration discovered inside a declaration container.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiscoveredDeclaration {
    pub(crate) kind: DeclarationKind,
    pub(crate) name: Option<DeclarationName>,
    pub(crate) syntax: SyntaxAnchor,
    pub(crate) surface: DeclarationSurface,
    pub(crate) children: Box<[DiscoveredDeclaration]>,
}

impl DiscoveredDeclaration {
    pub(crate) fn new(
        kind: DeclarationKind,
        name: Option<DeclarationName>,
        syntax: SyntaxAnchor,
        surface: DeclarationSurface,
        children: Box<[DiscoveredDeclaration]>,
    ) -> Self {
        Self {
            kind,
            name,
            syntax,
            surface,
            children,
        }
    }

    /// Returns this declaration's syntax-level kind.
    pub const fn kind(&self) -> DeclarationKind {
        self.kind
    }

    /// Returns this declaration's syntax-level name when one was present.
    pub const fn name(&self) -> Option<&DeclarationName> {
        self.name.as_ref()
    }

    /// Returns the source snapshot that contains this declaration.
    pub const fn source_id(&self) -> SourceId {
        self.syntax.source_id()
    }

    /// Returns the concrete syntax node kind this declaration came from.
    pub const fn syntax_kind(&self) -> bray_syntax::SyntaxKind {
        self.syntax.syntax_kind()
    }

    /// Returns the full source range covered by this declaration syntax.
    pub const fn full_range(&self) -> bray_source::TextRange {
        self.syntax.full_range()
    }

    /// Returns whether this declaration syntax contains parser recovery.
    pub const fn is_recovered(&self) -> bool {
        self.syntax.is_recovered()
    }

    /// Returns the stable syntax anchor for this declaration.
    pub const fn syntax_anchor(&self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns syntax-backed surface metadata for this declaration.
    pub const fn surface(&self) -> &DeclarationSurface {
        &self.surface
    }

    /// Returns direct declarations discovered inside this declaration's child container.
    pub fn children(&self) -> &[DiscoveredDeclaration] {
        &self.children
    }
}
