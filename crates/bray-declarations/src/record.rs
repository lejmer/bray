use crate::id::{ContainerId, DeclarationId, ModulePartId};
use crate::name::{DeclarationName, ModulePath};
use crate::surface::{DeclarationSurface, SyntaxAnchor};

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
    /// Struct field member declaration.
    StructField,
    /// Union variant member declaration.
    UnionVariant,
    /// Trait constant member declaration.
    TraitConstantMember,
    /// Trait type-valued member declaration.
    TraitTypeMember,
    /// Trait predicate member declaration.
    TraitPredicateMember,
    /// Trait callable member declaration.
    TraitCallableMember,
    /// Trait finalizer requirement declaration.
    TraitFinalizerRequirement,
    /// Trait destructor requirement declaration.
    TraitDestructorRequirement,
    /// Trait scope-enter requirement declaration.
    TraitScopeEnterRequirement,
    /// Trait scope-exit requirement declaration.
    TraitScopeExitRequirement,
    /// Implementation type-valued member binding.
    ImplementationTypeMemberBinding,
    /// Type constructor member declaration.
    TypeConstructorMember,
    /// Finalizer lifecycle member declaration.
    FinalizerMember,
    /// Destructor lifecycle member declaration.
    DestructorMember,
    /// Scope-enter lifecycle member declaration.
    ScopeEnterMember,
    /// Scope-exit lifecycle member declaration.
    ScopeExitMember,
    /// Type callable member declaration.
    TypeCallableMember,
    /// Generic type parameter declaration.
    GenericTypeParameter,
    /// Generic const parameter declaration.
    GenericConstParameter,
    /// Callable parameter declaration.
    CallableParameter,
    /// Predicate parameter declaration.
    PredicateParameter,
    /// Union variant payload field declaration.
    UnionPayloadField,
}

impl DeclarationKind {
    /// Returns this declaration kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Using => "using",
            Self::Export => "export",
            Self::Constant => "constant",
            Self::Function => "function",
            Self::Predicate => "predicate",
            Self::CallableContract => "callable_contract",
            Self::CallableOverload => "callable_overload",
            Self::ImplementationOverload => "implementation_overload",
            Self::Struct => "struct",
            Self::Union => "union",
            Self::Trait => "trait",
            Self::InherentImplementation => "inherent_implementation",
            Self::UnnamedTraitImplementation => "unnamed_trait_implementation",
            Self::NamedTraitImplementation => "named_trait_implementation",
            Self::StructField => "struct_field",
            Self::UnionVariant => "union_variant",
            Self::TraitConstantMember => "trait_constant_member",
            Self::TraitTypeMember => "trait_type_valued_member",
            Self::TraitPredicateMember => "trait_predicate_member",
            Self::TraitCallableMember => "trait_callable_member",
            Self::TraitFinalizerRequirement => "trait_finalizer_requirement",
            Self::TraitDestructorRequirement => "trait_destructor_requirement",
            Self::TraitScopeEnterRequirement => "trait_scope_enter_requirement",
            Self::TraitScopeExitRequirement => "trait_scope_exit_requirement",
            Self::ImplementationTypeMemberBinding => "implementation_type_valued_member_binding",
            Self::TypeConstructorMember => "type_constructor_member",
            Self::FinalizerMember => "finalizer_member",
            Self::DestructorMember => "destructor_member",
            Self::ScopeEnterMember => "scope_enter_member",
            Self::ScopeExitMember => "scope_exit_member",
            Self::TypeCallableMember => "type_callable_member",
            Self::GenericTypeParameter => "generic_type_parameter",
            Self::GenericConstParameter => "generic_const_parameter",
            Self::CallableParameter => "callable_parameter",
            Self::PredicateParameter => "predicate_parameter",
            Self::UnionPayloadField => "union_payload_field",
        }
    }

    /// Returns the child container category introduced by this declaration.
    pub const fn child_container_kind(self) -> Option<ContainerKind> {
        match self {
            Self::Struct | Self::Union => Some(ContainerKind::Type),
            Self::Trait => Some(ContainerKind::Trait),
            Self::InherentImplementation
            | Self::UnnamedTraitImplementation
            | Self::NamedTraitImplementation => Some(ContainerKind::Implementation),
            Self::CallableContract
            | Self::Function
            | Self::Predicate
            | Self::TraitPredicateMember
            | Self::TraitCallableMember
            | Self::TraitFinalizerRequirement
            | Self::TraitDestructorRequirement
            | Self::TraitScopeEnterRequirement
            | Self::TraitScopeExitRequirement
            | Self::TypeConstructorMember
            | Self::FinalizerMember
            | Self::DestructorMember
            | Self::ScopeEnterMember
            | Self::ScopeExitMember
            | Self::TypeCallableMember => Some(ContainerKind::Signature),
            Self::UnionVariant => Some(ContainerKind::Variant),
            Self::Module
            | Self::Using
            | Self::Export
            | Self::Constant
            | Self::CallableOverload
            | Self::ImplementationOverload
            | Self::StructField
            | Self::TraitConstantMember
            | Self::TraitTypeMember
            | Self::ImplementationTypeMemberBinding
            | Self::GenericTypeParameter
            | Self::GenericConstParameter
            | Self::CallableParameter
            | Self::PredicateParameter
            | Self::UnionPayloadField => None,
        }
    }

    pub(crate) const fn walks_child_declarations(self) -> bool {
        matches!(
            self,
            Self::Struct
                | Self::Union
                | Self::Trait
                | Self::InherentImplementation
                | Self::UnnamedTraitImplementation
                | Self::NamedTraitImplementation
        )
    }
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
    /// Callable, predicate, contract, or lifecycle signature container.
    Signature,
    /// Union variant payload container.
    Variant,
}

impl ContainerKind {
    /// Returns this container kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Module => "module",
            Self::Type => "type",
            Self::Trait => "trait",
            Self::Implementation => "implementation",
            Self::Signature => "signature",
            Self::Variant => "variant",
        }
    }
}

/// Immutable record for one discovered syntax declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DeclarationRecord {
    id: DeclarationId,
    kind: DeclarationKind,
    owning_container: ContainerId,
    name: Option<DeclarationName>,
    syntax: SyntaxAnchor,
    surface: DeclarationSurface,
    child_container: Option<ContainerId>,
}

impl DeclarationRecord {
    pub(crate) fn new(input: DeclarationRecordInput) -> Self {
        Self {
            id: input.id,
            kind: input.kind,
            owning_container: input.owning_container,
            name: input.name,
            syntax: input.syntax,
            surface: input.surface,
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
    pub const fn source_id(&self) -> bray_source::SourceId {
        self.syntax.source_id()
    }

    /// Returns the concrete syntax node kind this record came from.
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
    pub(crate) syntax: SyntaxAnchor,
    pub(crate) surface: DeclarationSurface,
    pub(crate) child_container: Option<ContainerId>,
}

/// Immutable record for one declaration container.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModulePartRecord {
    id: ModulePartId,
    module_container: ContainerId,
    declaration: DeclarationId,
    syntax: SyntaxAnchor,
    surface: DeclarationSurface,
    declarations: Box<[DeclarationId]>,
}

impl ModulePartRecord {
    pub(crate) fn new(input: ModulePartRecordInput) -> Self {
        Self {
            id: input.id,
            module_container: input.module_container,
            declaration: input.declaration,
            syntax: input.syntax,
            surface: input.surface,
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
    pub const fn source_id(&self) -> bray_source::SourceId {
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

    /// Returns declarations contributed by this module part in source order.
    pub fn declarations(&self) -> &[DeclarationId] {
        &self.declarations
    }
}

pub(crate) struct ModulePartRecordInput {
    pub(crate) id: ModulePartId,
    pub(crate) module_container: ContainerId,
    pub(crate) declaration: DeclarationId,
    pub(crate) syntax: SyntaxAnchor,
    pub(crate) surface: DeclarationSurface,
    pub(crate) declarations: Box<[DeclarationId]>,
}

#[cfg(test)]
mod tests {
    use super::{ContainerKind, DeclarationKind};

    #[test]
    fn declaration_and_container_kinds_have_stable_names() {
        assert_eq!(
            DeclarationKind::TraitTypeMember.as_str(),
            "trait_type_valued_member"
        );

        assert_eq!(ContainerKind::Signature.as_str(), "signature");
    }
}
