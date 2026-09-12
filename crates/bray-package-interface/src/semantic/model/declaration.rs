use std::sync::Arc;

use bray_symbols::{DeclaredCopyContract, DeclaredLayoutMode, IntegerConstant, ReceiverMode};

use super::InterfaceTypeId;
use crate::InterfaceSymbolReference;

/// One declaration's checked source-independent type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDeclaredType {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) ty: InterfaceTypeId,
}

impl InterfaceDeclaredType {
    /// Creates one declaration-owned type record.
    pub const fn new(owner: InterfaceSymbolReference, ty: InterfaceTypeId) -> Self {
        Self { owner, ty }
    }

    /// Returns the declaration owning this type.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the checked declared type.
    pub const fn ty(&self) -> InterfaceTypeId {
        self.ty
    }
}

/// One union variant tag in a declared type representation contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceUnionTag {
    pub(crate) variant: InterfaceSymbolReference,
    pub(crate) value: IntegerConstant,
}

/// One storage member required for consumer-side layout realization.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceStorageMember {
    pub(crate) field: Option<InterfaceSymbolReference>,
    pub(crate) ty: InterfaceTypeId,
}

impl InterfaceStorageMember {
    /// Creates one storage member with an optional exported field identity.
    pub const fn new(field: Option<InterfaceSymbolReference>, ty: InterfaceTypeId) -> Self {
        Self { field, ty }
    }

    /// Returns the exported field identity when one exists.
    pub const fn field(&self) -> Option<&InterfaceSymbolReference> {
        self.field.as_ref()
    }

    /// Returns the member type.
    pub const fn ty(&self) -> InterfaceTypeId {
        self.ty
    }
}

/// One union variant's payload storage shape.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceUnionStorageVariant {
    pub(crate) variant: InterfaceSymbolReference,
    pub(crate) members: Arc<[InterfaceStorageMember]>,
}

impl InterfaceUnionStorageVariant {
    /// Creates one variant payload shape in declaration order.
    pub fn new(
        variant: InterfaceSymbolReference,
        members: impl IntoIterator<Item = InterfaceStorageMember>,
    ) -> Self {
        Self {
            variant,
            members: members.into_iter().collect(),
        }
    }

    /// Returns the represented variant.
    pub const fn variant(&self) -> &InterfaceSymbolReference {
        &self.variant
    }

    /// Returns payload members in storage order.
    pub fn members(&self) -> &[InterfaceStorageMember] {
        &self.members
    }
}

/// A named type's storage shape without private declaration names.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceStorageShape {
    /// Product members in storage order.
    Structure(Arc<[InterfaceStorageMember]>),
    /// Union variants and payload members in declaration order.
    Union(Arc<[InterfaceUnionStorageVariant]>),
}

impl InterfaceUnionTag {
    /// Creates one checked union tag.
    pub const fn new(variant: InterfaceSymbolReference, value: IntegerConstant) -> Self {
        Self { variant, value }
    }

    /// Returns the tagged union variant.
    pub const fn variant(&self) -> &InterfaceSymbolReference {
        &self.variant
    }

    /// Returns the checked integer tag value.
    pub const fn value(&self) -> &IntegerConstant {
        &self.value
    }
}

/// Source-level representation semantics required by consumers of one declared type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceTypeRepresentation {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) layout: DeclaredLayoutMode,
    pub(crate) alignment: Option<u64>,
    pub(crate) packing: Option<u64>,
    pub(crate) opaque_size: Option<u64>,
    pub(crate) incomplete: bool,
    pub(crate) union_tag_type: Option<InterfaceTypeId>,
    pub(crate) tagless_union: bool,
    pub(crate) union_tags: Arc<[InterfaceUnionTag]>,
    pub(crate) storage: InterfaceStorageShape,
    pub(crate) copy: DeclaredCopyContract,
    pub(crate) copy_dependencies: Arc<[InterfaceSymbolReference]>,
    pub(crate) plain_storage: bool,
    pub(crate) finite_size: bool,
}

impl InterfaceTypeRepresentation {
    /// Creates one complete declared type representation contract.
    pub fn new(owner: InterfaceSymbolReference) -> Self {
        Self {
            owner,
            layout: DeclaredLayoutMode::Default,
            alignment: None,
            packing: None,
            opaque_size: None,
            incomplete: false,
            union_tag_type: None,
            tagless_union: false,
            union_tags: Arc::from([]),
            storage: InterfaceStorageShape::Structure(Arc::from([])),
            copy: DeclaredCopyContract::Absent,
            copy_dependencies: Arc::from([]),
            plain_storage: false,
            finite_size: false,
        }
    }

    /// Returns this contract with opaque and incomplete storage state.
    pub const fn with_opaque_storage(mut self, size: Option<u64>, incomplete: bool) -> Self {
        self.opaque_size = size;
        self.incomplete = incomplete;

        self
    }

    /// Returns this contract with an unrepresented semantic union tag.
    pub const fn with_tagless_union(mut self, tagless: bool) -> Self {
        self.tagless_union = tagless;

        self
    }

    /// Returns this contract with its checked layout request.
    pub const fn with_layout(
        mut self,
        layout: DeclaredLayoutMode,
        alignment: Option<u64>,
        packing: Option<u64>,
        union_tag_type: Option<InterfaceTypeId>,
    ) -> Self {
        self.layout = layout;
        self.alignment = alignment;
        self.packing = packing;
        self.union_tag_type = union_tag_type;

        self
    }

    /// Returns this contract with union tags in declaration order.
    pub fn with_union_tags(
        mut self,
        union_tags: impl IntoIterator<Item = InterfaceUnionTag>,
    ) -> Self {
        self.union_tags = union_tags.into_iter().collect();

        self
    }

    /// Returns this contract with its complete storage shape.
    pub fn with_storage(mut self, storage: InterfaceStorageShape) -> Self {
        self.storage = storage;

        self
    }

    /// Returns this contract with its implicit-copy policy and dependencies.
    pub fn with_copy(
        mut self,
        copy: DeclaredCopyContract,
        dependencies: impl IntoIterator<Item = InterfaceSymbolReference>,
    ) -> Self {
        self.copy = copy;
        self.copy_dependencies = dependencies.into_iter().collect();

        self
    }

    /// Returns this contract with its derived storage properties.
    pub const fn with_properties(mut self, plain_storage: bool, finite_size: bool) -> Self {
        self.plain_storage = plain_storage;
        self.finite_size = finite_size;

        self
    }

    /// Returns the declared type that owns this contract.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the selected source-level layout policy.
    pub const fn layout(&self) -> DeclaredLayoutMode {
        self.layout
    }

    /// Returns the requested minimum alignment.
    pub const fn alignment(&self) -> Option<u64> {
        self.alignment
    }

    /// Returns the requested maximum field alignment.
    pub const fn packing(&self) -> Option<u64> {
        self.packing
    }

    /// Returns the exact byte size of complete opaque storage.
    pub const fn opaque_size(&self) -> Option<u64> {
        self.opaque_size
    }

    /// Returns whether the type has no complete storage representation.
    pub const fn is_incomplete(&self) -> bool {
        self.incomplete
    }

    /// Returns the integer type used by union tags.
    pub const fn union_tag_type(&self) -> Option<InterfaceTypeId> {
        self.union_tag_type
    }

    /// Returns whether the union stores no represented discriminant.
    pub const fn is_tagless_union(&self) -> bool {
        self.tagless_union
    }

    /// Returns union tags in declaration order.
    pub fn union_tags(&self) -> &[InterfaceUnionTag] {
        &self.union_tags
    }

    /// Returns the complete storage shape used by consumers.
    pub const fn storage(&self) -> &InterfaceStorageShape {
        &self.storage
    }

    /// Returns the declared implicit-copy contract.
    pub const fn copy_contract(&self) -> DeclaredCopyContract {
        self.copy
    }

    /// Returns the generic type parameters whose arguments must be copyable.
    pub fn copy_dependencies(&self) -> &[InterfaceSymbolReference] {
        &self.copy_dependencies
    }

    /// Returns whether the type satisfies the plain-storage contract.
    pub const fn is_plain_storage(&self) -> bool {
        self.plain_storage
    }

    /// Returns whether the represented value has a finite outer size.
    pub const fn has_finite_size(&self) -> bool {
        self.finite_size
    }
}

/// One implicit receiver retained by a durable callable signature.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableReceiver {
    pub(crate) parameter: InterfaceSymbolReference,
    pub(crate) ty: InterfaceTypeId,
    pub(crate) mode: ReceiverMode,
}

impl InterfaceCallableReceiver {
    /// Creates one source-independent receiver signature.
    pub const fn new(
        parameter: InterfaceSymbolReference,
        ty: InterfaceTypeId,
        mode: ReceiverMode,
    ) -> Self {
        Self {
            parameter,
            ty,
            mode,
        }
    }

    /// Returns the exact receiver parameter.
    pub const fn parameter(&self) -> &InterfaceSymbolReference {
        &self.parameter
    }

    /// Returns the receiver's checked declared type.
    pub const fn ty(&self) -> InterfaceTypeId {
        self.ty
    }

    /// Returns the receiver's ownership and mutation mode.
    pub const fn mode(&self) -> ReceiverMode {
        self.mode
    }
}

/// One source-independent callable declaration signature.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableSignature {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) callable_type: InterfaceTypeId,
    pub(crate) receiver: Option<InterfaceCallableReceiver>,
    pub(crate) parameters: Arc<[InterfaceSymbolReference]>,
    pub(crate) result: InterfaceTypeId,
    pub(crate) has_body: bool,
}

impl InterfaceCallableSignature {
    /// Creates one durable callable signature in declaration parameter order.
    pub fn new(
        owner: InterfaceSymbolReference,
        callable_type: InterfaceTypeId,
        receiver: Option<InterfaceCallableReceiver>,
        parameters: impl IntoIterator<Item = InterfaceSymbolReference>,
        result: InterfaceTypeId,
    ) -> Self {
        Self {
            owner,
            callable_type,
            receiver,
            parameters: parameters.into_iter().collect(),
            result,
            has_body: false,
        }
    }

    /// Returns a signature with its declaration-body presence set.
    pub fn with_body(mut self, has_body: bool) -> Self {
        self.has_body = has_body;

        self
    }

    /// Returns the callable declaration that owns this signature.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the canonical callable type.
    pub const fn callable_type(&self) -> InterfaceTypeId {
        self.callable_type
    }

    /// Returns the implicit receiver when one is declared.
    pub const fn receiver(&self) -> Option<&InterfaceCallableReceiver> {
        self.receiver.as_ref()
    }

    /// Returns ordinary parameters in declaration order.
    pub fn parameters(&self) -> &[InterfaceSymbolReference] {
        &self.parameters
    }

    /// Returns the callable's checked declared result type.
    pub const fn result(&self) -> InterfaceTypeId {
        self.result
    }

    /// Returns whether the declaration supplies an executable body.
    pub const fn has_body(&self) -> bool {
        self.has_body
    }
}

/// One source-independent generic declaration surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceGenericDeclaration {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) parameters: Arc<[InterfaceSymbolReference]>,
}

impl InterfaceGenericDeclaration {
    /// Creates one durable generic declaration in parameter order.
    pub fn new(
        owner: InterfaceSymbolReference,
        parameters: impl IntoIterator<Item = InterfaceSymbolReference>,
    ) -> Self {
        Self {
            owner,
            parameters: parameters.into_iter().collect(),
        }
    }

    /// Returns the declaration that owns this generic surface.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns generic parameters in declaration order.
    pub fn parameters(&self) -> &[InterfaceSymbolReference] {
        &self.parameters
    }
}

/// One callable parameter's source-independent default-template presence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableParameterDefault {
    pub(crate) parameter: InterfaceSymbolReference,
    pub(crate) is_present: bool,
}

/// The validated definition form of one exported predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfacePredicateDefinitionState {
    /// The predicate has one checked definition template.
    Defined,
    /// A trait predicate member requires an implementation definition.
    Required,
    /// The predicate is an opaque trusted relation.
    OpaqueTrusted,
}

/// One source-independent predicate definition state.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfacePredicateDefinition {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) state: InterfacePredicateDefinitionState,
    pub(crate) parameters: Arc<
        [(
            InterfaceSymbolReference,
            bray_symbols::CallableParameterName,
            InterfaceTypeId,
        )],
    >,
}

impl InterfacePredicateDefinition {
    /// Creates one durable predicate definition record.
    pub fn new(owner: InterfaceSymbolReference, state: InterfacePredicateDefinitionState) -> Self {
        Self {
            owner,
            state,
            parameters: Arc::from([]),
        }
    }

    /// Retains the predicate's named parameters and resolved types in declaration order.
    pub fn with_parameters(
        mut self,
        parameters: impl IntoIterator<
            Item = (
                InterfaceSymbolReference,
                bray_symbols::CallableParameterName,
                InterfaceTypeId,
            ),
        >,
    ) -> Self {
        self.parameters = parameters.into_iter().collect();

        self
    }

    /// Returns the predicate signature in declaration order.
    pub fn parameters(
        &self,
    ) -> &[(
        InterfaceSymbolReference,
        bray_symbols::CallableParameterName,
        InterfaceTypeId,
    )] {
        &self.parameters
    }

    /// Returns the predicate declaration that owns this record.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the predicate's validated definition form.
    pub const fn state(&self) -> InterfacePredicateDefinitionState {
        self.state
    }
}

impl InterfaceCallableParameterDefault {
    /// Creates one durable callable parameter default record.
    pub const fn new(parameter: InterfaceSymbolReference, is_present: bool) -> Self {
        Self {
            parameter,
            is_present,
        }
    }

    /// Returns the exact callable parameter.
    pub const fn parameter(&self) -> &InterfaceSymbolReference {
        &self.parameter
    }

    /// Returns whether the declaration provides a default template.
    pub const fn is_present(&self) -> bool {
        self.is_present
    }
}
