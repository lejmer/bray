use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    GenericTypeParameterSymbolId, IntegerConstant, NamedTypeSymbolId, StructFieldSymbolId,
    TypeExpressionTemplate, TypeId, UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

/// One product-type storage member retained for layout realization.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredStructStorageMember {
    field: Option<StructFieldSymbolId>,
    ty: TypeExpressionTemplate,
}

impl DeclaredStructStorageMember {
    /// Creates one storage member with an optional consumer-visible field identity.
    pub const fn new(field: Option<StructFieldSymbolId>, ty: TypeExpressionTemplate) -> Self {
        Self { field, ty }
    }

    /// Returns the field identity when consumers may name this member.
    pub const fn field(&self) -> Option<StructFieldSymbolId> {
        self.field
    }

    /// Returns the member type before generic substitution.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }
}

/// One union payload storage member retained for layout realization.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredUnionStorageMember {
    field: Option<UnionPayloadFieldSymbolId>,
    ty: TypeExpressionTemplate,
}

impl DeclaredUnionStorageMember {
    /// Creates one payload member with an optional consumer-visible field identity.
    pub const fn new(field: Option<UnionPayloadFieldSymbolId>, ty: TypeExpressionTemplate) -> Self {
        Self { field, ty }
    }

    /// Returns the field identity when consumers may name this member.
    pub const fn field(&self) -> Option<UnionPayloadFieldSymbolId> {
        self.field
    }

    /// Returns the member type before generic substitution.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }
}

/// One union variant's payload storage shape.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredUnionStorageVariant {
    variant: UnionVariantSymbolId,
    members: Arc<[DeclaredUnionStorageMember]>,
}

impl DeclaredUnionStorageVariant {
    /// Creates one variant payload shape in declaration order.
    pub fn new(
        variant: UnionVariantSymbolId,
        members: impl IntoIterator<Item = DeclaredUnionStorageMember>,
    ) -> Self {
        Self {
            variant,
            members: shared_slice(members),
        }
    }

    /// Returns the represented variant.
    pub const fn variant(&self) -> UnionVariantSymbolId {
        self.variant
    }

    /// Returns payload members in storage order.
    pub fn members(&self) -> &[DeclaredUnionStorageMember] {
        &self.members
    }
}

/// A named type's complete storage shape without private source names.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclaredStorageShape {
    /// Product members in storage order.
    Structure(Arc<[DeclaredStructStorageMember]>),
    /// Union variants and their payload members in declaration order.
    Union(Arc<[DeclaredUnionStorageVariant]>),
}

impl DeclaredStorageShape {
    /// Visits every product or union payload member type in declaration order.
    pub fn member_types(&self) -> impl Iterator<Item = &TypeExpressionTemplate> {
        let (members, variants) = match self {
            Self::Structure(members) => (members.as_ref(), &[][..]),
            Self::Union(variants) => (&[][..], variants.as_ref()),
        };

        members.iter().map(DeclaredStructStorageMember::ty).chain(
            variants
                .iter()
                .flat_map(DeclaredUnionStorageVariant::members)
                .map(DeclaredUnionStorageMember::ty),
        )
    }
}

/// The source-level layout policy selected for one declared type.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclaredLayoutMode {
    /// Compiler-defined layout with no cross-build stability promise.
    #[default]
    Default,
    /// Bray's stable source-level layout contract.
    Stable,
    /// The selected target's C-compatible layout contract.
    C,
    /// The representation of a product type's single storage field.
    Transparent,
}

/// The copy contract derived for one declared type.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclaredCopyContract {
    /// The type does not declare implicit copying.
    #[default]
    Absent,
    /// Every valid instantiation of the type is implicitly copyable.
    Unconditional,
    /// Copyability depends on the type's generic arguments.
    Conditional,
}

/// One union variant's source-level tag value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredUnionTag {
    variant: UnionVariantSymbolId,
    value: IntegerConstant,
}

impl DeclaredUnionTag {
    /// Creates one checked union tag.
    pub const fn new(variant: UnionVariantSymbolId, value: IntegerConstant) -> Self {
        Self { variant, value }
    }

    /// Returns the tagged union variant.
    pub const fn variant(&self) -> UnionVariantSymbolId {
        self.variant
    }

    /// Returns the checked integer tag value.
    pub const fn value(&self) -> &IntegerConstant {
        &self.value
    }
}

/// The immutable source-level representation contract of one named type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredTypeRepresentation {
    subject: NamedTypeSymbolId,
    layout: DeclaredLayoutMode,
    alignment: Option<u64>,
    packing: Option<u64>,
    opaque_size: Option<u64>,
    incomplete: bool,
    union_tag_type: Option<TypeId>,
    tagless_union: bool,
    union_tags: Arc<[DeclaredUnionTag]>,
    storage: DeclaredStorageShape,
    copy: DeclaredCopyContract,
    copy_dependencies: Arc<[GenericTypeParameterSymbolId]>,
    plain_storage: bool,
    finite_size: bool,
    recovered: bool,
}

impl DeclaredTypeRepresentation {
    /// Creates the default contract for one named type.
    pub fn new(subject: NamedTypeSymbolId) -> Self {
        Self {
            subject,
            layout: DeclaredLayoutMode::Default,
            alignment: None,
            packing: None,
            opaque_size: None,
            incomplete: false,
            union_tag_type: None,
            tagless_union: false,
            union_tags: Arc::from([]),
            storage: match subject {
                NamedTypeSymbolId::Struct(_) => DeclaredStorageShape::Structure(Arc::from([])),
                NamedTypeSymbolId::Union(_) => DeclaredStorageShape::Union(Arc::from([])),
            },
            copy: DeclaredCopyContract::Absent,
            copy_dependencies: Arc::from([]),
            plain_storage: false,
            finite_size: false,
            recovered: false,
        }
    }

    /// Returns this contract with a checked opaque byte size.
    pub const fn with_opaque_size(mut self, size: Option<u64>) -> Self {
        self.opaque_size = size;

        self
    }

    /// Returns this contract with its incomplete-type state.
    pub const fn with_incomplete(mut self, incomplete: bool) -> Self {
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
        union_tag_type: Option<TypeId>,
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
        union_tags: impl IntoIterator<Item = DeclaredUnionTag>,
    ) -> Self {
        self.union_tags = shared_slice(union_tags);

        self
    }

    /// Returns this contract with its complete storage shape.
    pub fn with_storage(mut self, storage: DeclaredStorageShape) -> Self {
        self.storage = storage;

        self
    }

    /// Returns this contract with its derived representation properties.
    pub const fn with_properties(
        mut self,
        copy: DeclaredCopyContract,
        plain_storage: bool,
        finite_size: bool,
        recovered: bool,
    ) -> Self {
        self.copy = copy;
        self.plain_storage = plain_storage;
        self.finite_size = finite_size;
        self.recovered = recovered;

        self
    }

    /// Returns this contract with the type parameters that determine copyability.
    pub fn with_copy_dependencies(
        mut self,
        dependencies: impl IntoIterator<Item = GenericTypeParameterSymbolId>,
    ) -> Self {
        self.copy_dependencies = shared_slice(dependencies);

        self
    }

    /// Returns the named type owning this contract.
    pub const fn subject(&self) -> NamedTypeSymbolId {
        self.subject
    }

    /// Returns the selected source-level layout policy.
    pub const fn layout(&self) -> DeclaredLayoutMode {
        self.layout
    }

    /// Returns the requested minimum alignment when one was declared.
    pub const fn alignment(&self) -> Option<u64> {
        self.alignment
    }

    /// Returns the requested maximum field alignment when one was declared.
    pub const fn packing(&self) -> Option<u64> {
        self.packing
    }

    /// Returns the exact byte size of complete opaque storage.
    pub const fn opaque_size(&self) -> Option<u64> {
        self.opaque_size
    }

    /// Returns whether the declaration has no complete storage representation.
    pub const fn is_incomplete(&self) -> bool {
        self.incomplete
    }

    /// Returns the integer tag type fixed by the source-level union layout.
    pub const fn union_tag_type(&self) -> Option<TypeId> {
        self.union_tag_type
    }

    /// Returns whether the union stores no represented discriminant.
    pub const fn is_tagless_union(&self) -> bool {
        self.tagless_union
    }

    /// Returns union variant tags in declaration order.
    pub fn union_tags(&self) -> &[DeclaredUnionTag] {
        &self.union_tags
    }

    /// Returns the complete storage shape used for target layout realization.
    pub const fn storage(&self) -> &DeclaredStorageShape {
        &self.storage
    }

    /// Returns whether the structure ends in a flexible array member.
    pub fn has_flexible_trailing_member(&self) -> bool {
        matches!(
            &self.storage,
            DeclaredStorageShape::Structure(fields)
                if matches!(
                    fields.last().map(DeclaredStructStorageMember::ty),
                    Some(TypeExpressionTemplate::FlexibleArray(_))
                )
        )
    }

    /// Returns the type's checked implicit-copy contract.
    pub const fn copy_contract(&self) -> DeclaredCopyContract {
        self.copy
    }

    /// Returns the generic type parameters whose arguments must be copyable.
    pub fn copy_dependencies(&self) -> &[GenericTypeParameterSymbolId] {
        &self.copy_dependencies
    }

    /// Returns whether the representation satisfies Bray's plain-storage contract.
    pub const fn is_plain_storage(&self) -> bool {
        self.plain_storage
    }

    /// Returns whether every represented value has a known finite outer size.
    pub const fn has_finite_size(&self) -> bool {
        self.finite_size
    }

    /// Returns whether recovery affected the derived contract.
    pub const fn is_recovered(&self) -> bool {
        self.recovered
    }
}
