use bray_source::{SourceId, TextRange};
use bray_syntax::SyntaxKind;

use crate::id::{ContainerId, DeclarationId, ModulePartId};
use crate::name::{DeclarationName, ModulePath};

/// Syntax declaration category recorded before symbols exist.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationKind {
    /// Module declaration that contributes a module part.
    Module,
    /// `using` module-level declaration.
    Using,
    /// `export` module-level declaration.
    Export,
    /// Constant declaration.
    Constant,
    /// Function declaration.
    Function,
    /// Predicate declaration.
    Predicate,
    /// Callable contract declaration.
    CallableContract,
    /// Callable overload declaration.
    CallableOverload,
    /// Implementation overload declaration.
    ImplementationOverload,
    /// Struct declaration.
    Struct,
    /// Union declaration.
    Union,
    /// Trait declaration.
    Trait,
    /// Inherent implementation declaration.
    InherentImplementation,
    /// Unnamed trait implementation declaration.
    UnnamedTraitImplementation,
    /// Named trait implementation declaration.
    NamedTraitImplementation,
}

/// Declaration container category used before symbols exist.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ContainerKind {
    /// Compilation root container.
    Root,
    /// Logical module container.
    Module,
    /// Type body container.
    Type,
    /// Trait body container.
    Trait,
    /// Implementation body container.
    Implementation,
}

/// Immutable record for one discovered syntax declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationRecord {
    id: DeclarationId,
    kind: DeclarationKind,
    owning_container: ContainerId,
    name: Option<DeclarationName>,
    source_id: SourceId,
    syntax_kind: SyntaxKind,
    full_range: TextRange,
    is_recovered: bool,
    child_container: Option<ContainerId>,
}

impl DeclarationRecord {
    pub(crate) fn new(input: DeclarationRecordInput) -> Self {
        Self {
            id: input.id,
            kind: input.kind,
            owning_container: input.owning_container,
            name: input.name,
            source_id: input.source_id,
            syntax_kind: input.syntax_kind,
            full_range: input.full_range,
            is_recovered: input.is_recovered,
            child_container: input.child_container,
        }
    }

    /// Returns this declaration record's stable ID.
    pub const fn id(&self) -> DeclarationId {
        self.id
    }

    /// Returns this declaration's syntax-level kind.
    pub const fn kind(&self) -> DeclarationKind {
        self.kind
    }

    /// Returns the container that directly owns this declaration.
    pub const fn owning_container(&self) -> ContainerId {
        self.owning_container
    }

    /// Returns this declaration's syntax-level name when one was present.
    pub const fn name(&self) -> Option<&DeclarationName> {
        self.name.as_ref()
    }

    /// Returns the source snapshot that contains this declaration.
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns the concrete syntax node kind this record came from.
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

    /// Returns the child declaration container introduced by this declaration.
    pub const fn child_container(&self) -> Option<ContainerId> {
        self.child_container
    }
}

pub(crate) struct DeclarationRecordInput {
    pub(crate) id: DeclarationId,
    pub(crate) kind: DeclarationKind,
    pub(crate) owning_container: ContainerId,
    pub(crate) name: Option<DeclarationName>,
    pub(crate) source_id: SourceId,
    pub(crate) syntax_kind: SyntaxKind,
    pub(crate) full_range: TextRange,
    pub(crate) is_recovered: bool,
    pub(crate) child_container: Option<ContainerId>,
}

/// Immutable record for one declaration container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerRecord {
    id: ContainerId,
    kind: ContainerKind,
    parent: Option<ContainerId>,
    module_path: Option<ModulePath>,
    declarations: Box<[DeclarationId]>,
    module_parts: Box<[ModulePartId]>,
}

impl ContainerRecord {
    pub(crate) fn new(input: ContainerRecordInput) -> Self {
        Self {
            id: input.id,
            kind: input.kind,
            parent: input.parent,
            module_path: input.module_path,
            declarations: input.declarations,
            module_parts: input.module_parts,
        }
    }

    /// Returns this container's stable ID.
    pub const fn id(&self) -> ContainerId {
        self.id
    }

    /// Returns this container's syntax-level category.
    pub const fn kind(&self) -> ContainerKind {
        self.kind
    }

    /// Returns this container's parent container.
    pub const fn parent(&self) -> Option<ContainerId> {
        self.parent
    }

    /// Returns this container's module path when it is a module container.
    pub const fn module_path(&self) -> Option<&ModulePath> {
        self.module_path.as_ref()
    }

    /// Returns declarations directly owned by this container in stable order.
    pub fn declarations(&self) -> &[DeclarationId] {
        &self.declarations
    }

    /// Returns module parts that contribute to this module container.
    pub fn module_parts(&self) -> &[ModulePartId] {
        &self.module_parts
    }
}

pub(crate) struct ContainerRecordInput {
    pub(crate) id: ContainerId,
    pub(crate) kind: ContainerKind,
    pub(crate) parent: Option<ContainerId>,
    pub(crate) module_path: Option<ModulePath>,
    pub(crate) declarations: Box<[DeclarationId]>,
    pub(crate) module_parts: Box<[ModulePartId]>,
}

/// Immutable record for one syntax contribution to a partial module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModulePartRecord {
    id: ModulePartId,
    module_container: ContainerId,
    declaration: DeclarationId,
    source_id: SourceId,
    syntax_kind: SyntaxKind,
    full_range: TextRange,
    is_recovered: bool,
    declarations: Box<[DeclarationId]>,
}

impl ModulePartRecord {
    pub(crate) fn new(input: ModulePartRecordInput) -> Self {
        Self {
            id: input.id,
            module_container: input.module_container,
            declaration: input.declaration,
            source_id: input.source_id,
            syntax_kind: input.syntax_kind,
            full_range: input.full_range,
            is_recovered: input.is_recovered,
            declarations: input.declarations,
        }
    }

    /// Returns this module part's stable ID.
    pub const fn id(&self) -> ModulePartId {
        self.id
    }

    /// Returns the logical module container this source part contributes to.
    pub const fn module_container(&self) -> ContainerId {
        self.module_container
    }

    /// Returns the module declaration record for this part.
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
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

    /// Returns declarations contributed by this module part in source order.
    pub fn declarations(&self) -> &[DeclarationId] {
        &self.declarations
    }
}

pub(crate) struct ModulePartRecordInput {
    pub(crate) id: ModulePartId,
    pub(crate) module_container: ContainerId,
    pub(crate) declaration: DeclarationId,
    pub(crate) source_id: SourceId,
    pub(crate) syntax_kind: SyntaxKind,
    pub(crate) full_range: TextRange,
    pub(crate) is_recovered: bool,
    pub(crate) declarations: Box<[DeclarationId]>,
}
