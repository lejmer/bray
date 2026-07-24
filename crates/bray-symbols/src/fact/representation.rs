use std::sync::Arc;

use bray_base::shared_slice;

use crate::{IntegerConstant, NamedTypeSymbolId, TypeId, UnionVariantSymbolId};

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
    union_tag_type: Option<TypeId>,
    union_tags: Arc<[DeclaredUnionTag]>,
    copy: DeclaredCopyContract,
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
            union_tag_type: None,
            union_tags: Arc::from([]),
            copy: DeclaredCopyContract::Absent,
            plain_storage: false,
            finite_size: false,
            recovered: false,
        }
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

    /// Returns the integer tag type fixed by the source-level union layout.
    pub const fn union_tag_type(&self) -> Option<TypeId> {
        self.union_tag_type
    }

    /// Returns union variant tags in declaration order.
    pub fn union_tags(&self) -> &[DeclaredUnionTag] {
        &self.union_tags
    }

    /// Returns the type's checked implicit-copy contract.
    pub const fn copy_contract(&self) -> DeclaredCopyContract {
        self.copy
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
