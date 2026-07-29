use std::sync::Arc;

use bray_base::shared_slice;
use bray_ir::MirFieldReference;
use bray_symbols::{CallableAbi, TypeId, UnionVariantSymbolId};
use bray_target::TargetValueLayout;

use crate::{TargetAddressSpaceKind, TargetScalarKind};

/// Complete machine signature selected for one callable address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCallableSignature {
    parameters: Arc<[TypeId]>,
    result: TypeId,
    abi: CallableAbi,
}

impl CodegenCallableSignature {
    /// Creates a callable signature from ordered parameters, result, and ABI.
    pub fn new(
        parameters: impl IntoIterator<Item = TypeId>,
        result: TypeId,
        abi: CallableAbi,
    ) -> Self {
        Self {
            parameters: shared_slice(parameters),
            result,
            abi,
        }
    }

    /// Returns parameters in call order.
    pub fn parameters(&self) -> &[TypeId] {
        &self.parameters
    }

    /// Returns the callable result representation.
    pub const fn result(&self) -> TypeId {
        self.result
    }

    /// Returns the selected callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }
}

/// Exact physical layout of one aggregate field.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenFieldLayout {
    reference: Option<MirFieldReference>,
    ty: TypeId,
    offset_bytes: u64,
}

impl CodegenFieldLayout {
    /// Creates one field layout in declaration or structural order.
    pub const fn new(reference: Option<MirFieldReference>, ty: TypeId, offset_bytes: u64) -> Self {
        Self {
            reference,
            ty,
            offset_bytes,
        }
    }

    /// Returns the declared field identity, when the field is declaration-backed.
    pub const fn reference(&self) -> Option<MirFieldReference> {
        self.reference
    }

    /// Returns the exact represented field type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the field's byte offset from the aggregate base.
    pub const fn offset_bytes(&self) -> u64 {
        self.offset_bytes
    }
}

/// Exact physical layout of one union variant payload.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenUnionVariantLayout {
    variant: UnionVariantSymbolId,
    tag: u128,
    fields: Arc<[CodegenFieldLayout]>,
}

impl CodegenUnionVariantLayout {
    /// Creates one tagged union payload layout.
    pub fn new(
        variant: UnionVariantSymbolId,
        tag: u128,
        fields: impl IntoIterator<Item = CodegenFieldLayout>,
    ) -> Self {
        Self {
            variant,
            tag,
            fields: shared_slice(fields),
        }
    }

    /// Returns the selected union variant.
    pub const fn variant(&self) -> UnionVariantSymbolId {
        self.variant
    }

    /// Returns the checked integer tag value.
    pub const fn tag(&self) -> u128 {
        self.tag
    }

    /// Returns payload fields in declaration order.
    pub fn fields(&self) -> &[CodegenFieldLayout] {
        &self.fields
    }
}

/// Backend-neutral physical representation selected for one semantic type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenTypeKind {
    /// The zero-sized unit representation.
    Unit,
    /// One target scalar representation.
    Scalar(TargetScalarKind),
    /// One pointer in a selected target address space.
    Pointer {
        /// The represented pointee type.
        target: TypeId,
        /// The language-level address-space role.
        address_space: TargetAddressSpaceKind,
    },
    /// An ordered aggregate representation.
    Aggregate(Arc<[CodegenFieldLayout]>),
    /// A fixed-size homogeneous array.
    Array {
        /// Element representation.
        element: TypeId,
        /// Checked element count.
        length: u64,
    },
    /// A tagged union representation.
    Union {
        /// Integer type used for the active tag.
        tag: TypeId,
        /// Variant payload layouts in declaration order.
        variants: Arc<[CodegenUnionVariantLayout]>,
    },
    /// A callable address with its complete machine signature.
    Callable(Arc<CodegenCallableSignature>),
}

impl CodegenTypeKind {
    /// Creates an ordered aggregate representation.
    pub fn aggregate(fields: impl IntoIterator<Item = CodegenFieldLayout>) -> Self {
        Self::Aggregate(shared_slice(fields))
    }

    /// Creates a tagged union representation.
    pub fn union(
        tag: TypeId,
        variants: impl IntoIterator<Item = CodegenUnionVariantLayout>,
    ) -> Self {
        Self::Union {
            tag,
            variants: shared_slice(variants),
        }
    }

    /// Creates a callable representation.
    pub fn callable(
        parameters: impl IntoIterator<Item = TypeId>,
        result: TypeId,
        abi: CallableAbi,
    ) -> Self {
        Self::Callable(Arc::new(CodegenCallableSignature::new(
            parameters, result, abi,
        )))
    }
}

/// Exact target layout and representation of one demanded semantic type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenTypeMapping {
    ty: TypeId,
    layout: TargetValueLayout,
    kind: CodegenTypeKind,
}

impl CodegenTypeMapping {
    /// Creates one completed type mapping.
    pub const fn new(ty: TypeId, layout: TargetValueLayout, kind: CodegenTypeKind) -> Self {
        Self { ty, layout, kind }
    }

    /// Returns the semantic type identity.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the exact selected physical layout.
    pub const fn layout(&self) -> TargetValueLayout {
        self.layout
    }

    /// Returns the backend-neutral physical representation.
    pub const fn kind(&self) -> &CodegenTypeKind {
        &self.kind
    }
}
