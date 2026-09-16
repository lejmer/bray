use std::num::{NonZeroU16, NonZeroU64};
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_ir::MirFieldReference;
use bray_symbols::{CallableAbi, IntegerConstant, TypeId, UnionVariantSymbolId};
use bray_target::TargetValueLayout;

use crate::{CodegenInstanceKey, TargetAddressSpaceKind};

/// One concrete type selected for an open MIR type in a code generation instance.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenInstanceTypeMapping {
    instance: CodegenInstanceKey,
    template: TypeId,
    concrete: TypeId,
}

impl CodegenInstanceTypeMapping {
    /// Creates one instance-local type substitution.
    pub const fn new(instance: CodegenInstanceKey, template: TypeId, concrete: TypeId) -> Self {
        Self {
            instance,
            template,
            concrete,
        }
    }

    /// Returns the concrete code generation instance.
    pub const fn instance(&self) -> &CodegenInstanceKey {
        &self.instance
    }

    /// Returns the open type retained by the MIR template.
    pub const fn template(&self) -> TypeId {
        self.template
    }

    /// Returns the closed semantic type selected for the instance.
    pub const fn concrete(&self) -> TypeId {
        self.concrete
    }
}

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
        /// Pointee representation named by the target ABI attribute.
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

}

/// Complete target-classified machine signature selected for one callable address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCallableSignature {
    parameters: Arc<[CodegenParameterMapping]>,
    result: CodegenResultMapping,
    abi: CallableAbi,
    variadic: bool,
    panic_report_context: bool,
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
            panic_report_context: false,
        }
    }

    /// Adds the hidden context through which a synchronous Bray call returns a panic report.
    pub const fn with_panic_report_context(mut self) -> Self {
        self.panic_report_context = true;

        self
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

    /// Returns whether the physical signature carries a hidden panic-report context.
    pub const fn has_panic_report_context(&self) -> bool {
        self.panic_report_context
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
    tag: Option<IntegerConstant>,
    fields: Arc<[CodegenFieldLayout]>,
}

impl CodegenUnionVariantLayout {
    /// Creates one tagged union payload layout.
    pub fn new(
        variant: UnionVariantSymbolId,
        tag: IntegerConstant,
        fields: impl IntoIterator<Item = CodegenFieldLayout>,
    ) -> Self {
        Self {
            variant,
            tag: Some(tag),
            fields: shared_slice(fields),
        }
    }

    /// Creates one untagged union payload layout.
    pub fn untagged(
        variant: UnionVariantSymbolId,
        fields: impl IntoIterator<Item = CodegenFieldLayout>,
    ) -> Self {
        Self {
            variant,
            tag: None,
            fields: shared_slice(fields),
        }
    }

    /// Returns the selected union variant.
    pub const fn variant(&self) -> UnionVariantSymbolId {
        self.variant
    }

    /// Returns the checked integer tag value.
    pub const fn tag(&self) -> Option<&IntegerConstant> {
        self.tag.as_ref()
    }

    /// Returns payload fields in declaration order.
    pub fn fields(&self) -> &[CodegenFieldLayout] {
        &self.fields
    }

    /// Returns the physical layout of one declared payload field.
    pub fn payload_field(
        &self,
        field: bray_symbols::UnionPayloadFieldSymbolId,
    ) -> Option<&CodegenFieldLayout> {
        self.fields
            .iter()
            .find(|layout| layout.reference() == Some(MirFieldReference::UnionPayload(field)))
    }

    /// Returns a represented payload member, including members hidden from source lookup.
    pub fn payload_element(
        &self,
        ordinal: bray_symbols::SymbolOrdinal,
    ) -> Option<&CodegenFieldLayout> {
        self.fields.get(usize::try_from(ordinal.raw()).ok()?)
    }
}

/// Backend-neutral physical representation selected for one semantic type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenTypeKind {
    /// The zero-sized unit representation.
    Unit,
    /// The target Boolean representation.
    Boolean,
    /// One signed integer representation.
    SignedInteger(NonZeroU16),
    /// One unsigned integer representation.
    UnsignedInteger(NonZeroU16),
    /// One floating-point representation.
    Float(NonZeroU16),
    /// One pointer in a selected target address space.
    Pointer {
        /// The represented pointee type.
        target: TypeId,
        /// The language-level address-space role.
        address_space: TargetAddressSpaceKind,
    },
    /// An ordered aggregate representation.
    Aggregate(Arc<[CodegenFieldLayout]>),
    /// An incomplete native type that is representable only behind an indirection.
    Opaque,
    /// A fixed-size homogeneous array.
    Array {
        /// Element representation.
        element: TypeId,
        /// Checked element count.
        length: u64,
    },
    /// An unsized contiguous sequence whose runtime length is carried by an indirection.
    UnsizedSlice {
        /// Element representation.
        element: TypeId,
    },
    /// An unsized trait view whose implementation witness is carried by an indirection.
    UnsizedTraitView,
    /// A tagged union representation.
    Union {
        /// Integer type used for the represented active tag.
        tag: Option<TypeId>,
        /// Variant payload layouts in declaration order.
        variants: Arc<[CodegenUnionVariantLayout]>,
    },
    /// A callable address with its complete machine signature.
    Callable(Arc<CodegenCallableSignature>),
}

/// Semantic behavior attached to one protected physical representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenTypeBehavior {
    /// An owning panic header shared with the native runtime ABI.
    PanicReport,
    /// Immutable UTF-8 text with shared owned storage.
    String,
    /// A C-layout product with one flexible trailing element sequence.
    FlexibleAggregate {
        /// Trailing element representation.
        element: TypeId,
        /// Byte offset at which the trailing sequence begins.
        offset: u64,
    },
}

impl CodegenTypeKind {
    /// Returns one union variant's physical layout, or none for a different representation or variant.
    pub fn union_variant(
        &self,
        variant: UnionVariantSymbolId,
    ) -> Option<&CodegenUnionVariantLayout> {
        let Self::Union { variants, .. } = self else {
            return None;
        };

        variants.iter().find(|layout| layout.variant() == variant)
    }

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
            tag: Some(tag),
            variants: shared_slice(variants),
        }
    }

    /// Creates an overlapping union representation without a stored tag.
    pub fn untagged_union(variants: impl IntoIterator<Item = CodegenUnionVariantLayout>) -> Self {
        Self::Union {
            tag: None,
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
    backend_type: TypeId,
    layout: Option<TargetValueLayout>,
    kind: CodegenTypeKind,
    behavior: Option<CodegenTypeBehavior>,
}

impl CodegenTypeMapping {
    /// Creates one completed sized type mapping.
    pub const fn new(ty: TypeId, layout: TargetValueLayout, kind: CodegenTypeKind) -> Self {
        Self {
            ty,
            backend_type: ty,
            layout: Some(layout),
            kind,
            behavior: None,
        }
    }

    /// Creates one completed unsized type mapping.
    pub const fn new_unsized(ty: TypeId, kind: CodegenTypeKind) -> Self {
        Self {
            ty,
            backend_type: ty,
            layout: None,
            kind,
            behavior: None,
        }
    }

    /// Shares this physical representation with another semantic type, without its behavior.
    pub fn representation_for(&self, ty: TypeId) -> Self {
        Self {
            ty,
            backend_type: self.backend_type,
            layout: self.layout,
            // Each semantic mapping owns its descriptor while aggregate members remain shared.
            kind: self.kind.clone(),
            behavior: None,
        }
    }

    /// Returns this mapping using another semantic type's exact backend identity.
    pub const fn with_backend_type(mut self, backend_type: TypeId) -> Self {
        self.backend_type = backend_type;

        self
    }

    /// Returns this mapping with its optional semantic behavior.
    pub const fn with_behavior(mut self, behavior: Option<CodegenTypeBehavior>) -> Self {
        self.behavior = behavior;

        self
    }

    /// Returns the semantic type identity.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the semantic type whose backend identity represents this type.
    pub const fn backend_type(&self) -> TypeId {
        self.backend_type
    }

    /// Returns the exact selected physical layout, or `None` for an unsized semantic type.
    pub const fn layout(&self) -> Option<TargetValueLayout> {
        self.layout
    }

    /// Returns the backend-neutral physical representation.
    pub const fn kind(&self) -> &CodegenTypeKind {
        &self.kind
    }

    /// Returns semantic behavior attached to this representation.
    pub const fn behavior(&self) -> Option<CodegenTypeBehavior> {
        self.behavior
    }
}

#[cfg(test)]
mod tests {
    use super::{CodegenTypeBehavior, CodegenTypeKind, CodegenTypeMapping};
    use bray_symbols::{SemanticValueStore, TypeData};

    #[test]
    fn union_member_ordinals_preserve_hidden_storage_and_reject_unknown_members() {
        use super::{CodegenFieldLayout, CodegenUnionVariantLayout};

        use bray_symbols::{
            SymbolId, SymbolOrdinal, UnionPayloadFieldSymbolId, UnionVariantSymbolId,
        };

        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();
        let variant = UnionVariantSymbolId::from_symbol_id(SymbolId::new(1));
        let field = UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(2));

        let kind = CodegenTypeKind::untagged_union([CodegenUnionVariantLayout::untagged(
            variant,
            [
                CodegenFieldLayout::new(
                    Some(bray_ir::MirFieldReference::UnionPayload(field)),
                    ty,
                    8,
                ),
                CodegenFieldLayout::new(None, ty, 24),
            ],
        )]);

        let layout = kind.union_variant(variant).unwrap();

        assert_eq!(
            layout.payload_element(SymbolOrdinal::new(0)),
            layout.payload_field(field)
        );

        assert_eq!(
            layout.payload_element(SymbolOrdinal::new(1)),
            Some(&CodegenFieldLayout::new(None, ty, 24))
        );

        assert_eq!(layout.payload_element(SymbolOrdinal::new(2)), None);
        assert_eq!(layout.payload_element(SymbolOrdinal::new(u32::MAX)), None);

        assert_eq!(
            kind.union_variant(UnionVariantSymbolId::from_symbol_id(SymbolId::new(3))),
            None
        );

        assert_eq!(CodegenTypeKind::Boolean.union_variant(variant), None);
    }

    #[test]
    fn shared_representation_preserves_backend_identity_without_semantic_behavior() {
        let values = SemanticValueStore::try_new().unwrap();
        let original = values.intern_type(TypeData::tuple([])).unwrap();
        let backend = values.intern_type(TypeData::Slice(original)).unwrap();
        let alias = values.intern_type(TypeData::Nullable(original)).unwrap();

        let mapping = CodegenTypeMapping::new_unsized(original, CodegenTypeKind::UnsizedTraitView)
            .with_backend_type(backend)
            .with_behavior(Some(CodegenTypeBehavior::String));

        assert_eq!(
            mapping.representation_for(alias),
            CodegenTypeMapping::new_unsized(alias, CodegenTypeKind::UnsizedTraitView)
                .with_backend_type(backend)
        );

        assert_eq!(mapping.behavior(), Some(CodegenTypeBehavior::String));
    }
}
