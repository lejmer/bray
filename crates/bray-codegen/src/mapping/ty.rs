use std::num::NonZeroU64;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_ir::MirFieldReference;
use bray_symbols::{CallableAbi, TypeId, UnionVariantSymbolId};
use bray_target::TargetValueLayout;

use crate::{TargetAddressSpaceKind, TargetScalarKind};

/// Integer extension selected for a directly passed ABI value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenIntegerExtension {
    /// Sign-extend the value at the call boundary.
    Sign,
    /// Zero-extend the value at the call boundary.
    Zero,
}

/// Independently proven ABI attribute for one passed value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenValueAttribute {
    /// Prefer a target register for the value.
    InRegister,
    /// The pointer does not alias another pointer visible to the call.
    NoAlias,
    /// The value may not be null.
    NonNull,
    /// The value may not contain poison or undefined bits.
    NoUndef,
}

/// Indirect parameter semantics selected by target ABI classification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenIndirectParameterKind {
    /// Pass an ordinary pointer to caller-owned storage.
    Reference,
    /// Pass a pointer to a callee-visible copy of the value.
    ByValue,
}

/// Machine passing mode selected for one semantic parameter position.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenParameterMapping {
    /// Omit this semantic parameter from the machine signature.
    Ignore,
    /// Pass one direct machine value.
    Direct {
        /// Machine representation passed to the callee.
        ty: TypeId,
        /// Integer extension required at the call boundary.
        extension: Option<CodegenIntegerExtension>,
        /// Independently proven value attributes.
        attributes: Arc<[CodegenValueAttribute]>,
    },
    /// Pass one pointer to the semantic value.
    Indirect {
        /// Machine pointer representation.
        pointer: TypeId,
        /// Pointee representation named by the LLVM ABI attribute.
        pointee: TypeId,
        /// Reference or by-value indirect semantics.
        kind: CodegenIndirectParameterKind,
        /// Required pointee alignment in bytes.
        alignment: NonZeroU64,
        /// Independently proven pointer attributes.
        attributes: Arc<[CodegenValueAttribute]>,
    },
}

impl CodegenParameterMapping {
    /// Creates one direct parameter mapping.
    pub fn direct(
        ty: TypeId,
        extension: Option<CodegenIntegerExtension>,
        attributes: impl IntoIterator<Item = CodegenValueAttribute>,
    ) -> Self {
        Self::Direct {
            ty,
            extension,
            attributes: sorted_unique_shared_slice(attributes),
        }
    }

    /// Creates one indirect parameter mapping.
    pub fn indirect(
        pointer: TypeId,
        pointee: TypeId,
        kind: CodegenIndirectParameterKind,
        alignment: NonZeroU64,
        attributes: impl IntoIterator<Item = CodegenValueAttribute>,
    ) -> Self {
        Self::Indirect {
            pointer,
            pointee,
            kind,
            alignment,
            attributes: sorted_unique_shared_slice(attributes),
        }
    }

    pub(crate) fn demanded_types(&self) -> [Option<TypeId>; 2] {
        match self {
            Self::Ignore => [None, None],
            Self::Direct { ty, .. } => [Some(*ty), None],
            Self::Indirect {
                pointer, pointee, ..
            } => [Some(*pointer), Some(*pointee)],
        }
    }
}

/// Machine passing mode selected for one callable result.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenResultMapping {
    /// Return no machine value.
    Void,
    /// Return one direct machine value.
    Direct {
        /// Machine representation returned to the caller.
        ty: TypeId,
        /// Integer extension required at the call boundary.
        extension: Option<CodegenIntegerExtension>,
        /// Independently proven result attributes.
        attributes: Arc<[CodegenValueAttribute]>,
    },
    /// Return through a hidden caller-provided pointer.
    Indirect {
        /// Machine pointer representation of the hidden parameter.
        pointer: TypeId,
        /// Pointee representation named by the structure-return attribute.
        pointee: TypeId,
        /// Required pointee alignment in bytes.
        alignment: NonZeroU64,
        /// Independently proven hidden-pointer attributes.
        attributes: Arc<[CodegenValueAttribute]>,
    },
}

impl CodegenResultMapping {
    /// Creates one direct result mapping.
    pub fn direct(
        ty: TypeId,
        extension: Option<CodegenIntegerExtension>,
        attributes: impl IntoIterator<Item = CodegenValueAttribute>,
    ) -> Self {
        Self::Direct {
            ty,
            extension,
            attributes: sorted_unique_shared_slice(attributes),
        }
    }

    /// Creates one indirect result mapping.
    pub fn indirect(
        pointer: TypeId,
        pointee: TypeId,
        alignment: NonZeroU64,
        attributes: impl IntoIterator<Item = CodegenValueAttribute>,
    ) -> Self {
        Self::Indirect {
            pointer,
            pointee,
            alignment,
            attributes: sorted_unique_shared_slice(attributes),
        }
    }

    pub(crate) fn demanded_types(&self) -> [Option<TypeId>; 2] {
        match self {
            Self::Void => [None, None],
            Self::Direct { ty, .. } => [Some(*ty), None],
            Self::Indirect {
                pointer, pointee, ..
            } => [Some(*pointer), Some(*pointee)],
        }
    }
}

/// Complete target-classified machine signature selected for one callable address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCallableSignature {
    parameters: Arc<[CodegenParameterMapping]>,
    result: CodegenResultMapping,
    abi: CallableAbi,
    variadic: bool,
}

impl CodegenCallableSignature {
    /// Creates a callable signature from ordered passing modes, result, ABI, and variadic shape.
    pub fn new(
        parameters: impl IntoIterator<Item = CodegenParameterMapping>,
        result: CodegenResultMapping,
        abi: CallableAbi,
        variadic: bool,
    ) -> Self {
        Self {
            parameters: shared_slice(parameters),
            result,
            abi,
            variadic,
        }
    }

    /// Returns target-classified parameters in semantic call order.
    pub fn parameters(&self) -> &[CodegenParameterMapping] {
        &self.parameters
    }

    /// Returns the target-classified result.
    pub const fn result(&self) -> &CodegenResultMapping {
        &self.result
    }

    /// Returns the selected callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns whether the machine signature accepts trailing variadic arguments.
    pub const fn is_variadic(&self) -> bool {
        self.variadic
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
        parameters: impl IntoIterator<Item = CodegenParameterMapping>,
        result: CodegenResultMapping,
        abi: CallableAbi,
        variadic: bool,
    ) -> Self {
        Self::Callable(Arc::new(CodegenCallableSignature::new(
            parameters, result, abi, variadic,
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
