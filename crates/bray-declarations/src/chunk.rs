use bray_source::{SourceId, TextRange};
use bray_syntax::SyntaxKind;

use crate::name::{DeclarationName, ModulePath};
use crate::record::DeclarationKind;

/// Immutable declaration discovery output for one source unit.
#[derive(Clone, Debug, Eq, PartialEq)]
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

    pub(crate) fn into_module_parts(self) -> Box<[DiscoveredModulePart]> {
        self.module_parts
    }
}

/// Immutable source-unit contribution to a logical module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredModulePart {
    pub(crate) path: ModulePath,
    pub(crate) source_id: SourceId,
    pub(crate) syntax_kind: SyntaxKind,
    pub(crate) full_range: TextRange,
    pub(crate) is_recovered: bool,
    pub(crate) declarations: Box<[DiscoveredDeclaration]>,
}

impl DiscoveredModulePart {
    pub(crate) fn new(
        path: ModulePath,
        source_id: SourceId,
        syntax_kind: SyntaxKind,
        full_range: TextRange,
        is_recovered: bool,
        declarations: Box<[DiscoveredDeclaration]>,
    ) -> Self {
        Self {
            path,
            source_id,
            syntax_kind,
            full_range,
            is_recovered,
            declarations,
        }
    }

    /// Returns the syntactic module path for this part.
    pub const fn path(&self) -> &ModulePath {
        &self.path
    }

    /// Returns the source snapshot that contains this module part.
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns the concrete syntax node kind this module part came from.
    pub const fn syntax_kind(&self) -> SyntaxKind {
        self.syntax_kind
    }

    /// Returns the full source range covered by this module declaration syntax.
    pub const fn full_range(&self) -> TextRange {
        self.full_range
    }

    /// Returns whether this module part syntax contains parser recovery.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Returns module-level declarations contributed by this part in source order.
    pub fn declarations(&self) -> &[DiscoveredDeclaration] {
        &self.declarations
    }
}

/// Immutable source-order declaration discovered inside a declaration container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredDeclaration {
    pub(crate) kind: DeclarationKind,
    pub(crate) name: Option<DeclarationName>,
    pub(crate) source_id: SourceId,
    pub(crate) syntax_kind: SyntaxKind,
    pub(crate) full_range: TextRange,
    pub(crate) is_recovered: bool,
    pub(crate) children: Box<[DiscoveredDeclaration]>,
}

impl DiscoveredDeclaration {
    pub(crate) fn new(
        kind: DeclarationKind,
        name: Option<DeclarationName>,
        source_id: SourceId,
        syntax_kind: SyntaxKind,
        full_range: TextRange,
        is_recovered: bool,
        children: Box<[DiscoveredDeclaration]>,
    ) -> Self {
        Self {
            kind,
            name,
            source_id,
            syntax_kind,
            full_range,
            is_recovered,
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
        self.source_id
    }

    /// Returns the concrete syntax node kind this declaration came from.
    pub const fn syntax_kind(&self) -> SyntaxKind {
        self.syntax_kind
    }

    /// Returns the full source range covered by this declaration syntax.
    pub const fn full_range(&self) -> TextRange {
        self.full_range
    }

    /// Returns whether this declaration syntax contains parser recovery.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Returns direct declarations discovered inside this declaration's child container.
    pub fn children(&self) -> &[DiscoveredDeclaration] {
        &self.children
    }
}
