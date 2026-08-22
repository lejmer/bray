/// Named option within a source-level type layout contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLayoutOption {
    /// Explicit storage alignment.
    Alignment,
    /// Packed stable storage.
    Packing,
    /// Exact opaque storage size.
    Size,
    /// Explicit union-tag type.
    Tag,
}

impl DiagnosticLayoutOption {
    /// Returns the stable machine key for this layout option.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alignment => "alignment",
            Self::Packing => "packing",
            Self::Size => "size",
            Self::Tag => "tag",
        }
    }
}

/// Exact reason a declared layout contract cannot be honored.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLayoutProblem {
    /// The directive omitted its required layout mode.
    MissingMode,
    /// The supplied layout-mode spelling is not recognized.
    UnsupportedMode(String),
    /// A named option was supplied more than once.
    DuplicateOption(DiagnosticLayoutOption),
    /// The supplied named option is not part of the layout contract.
    UnknownOption(String),
    /// A positional argument appeared after the layout mode.
    UnexpectedPositionalArgument,
    /// An option value did not evaluate to a nonnegative integer.
    OptionNotConstant(DiagnosticLayoutOption),
    /// An option value is not a power of two.
    OptionNotPowerOfTwo {
        /// The rejected option.
        option: DiagnosticLayoutOption,
        /// The evaluated nonnegative value.
        value: u64,
    },
    /// Transparent layout was applied to a union.
    TransparentUnion,
    /// Transparent layout did not contain exactly one field.
    TransparentFieldCount {
        /// Number of declared fields.
        actual: u64,
    },
    /// Transparent layout also selected an incompatible layout option.
    TransparentOption(DiagnosticLayoutOption),
    /// Packed layout did not use stable layout mode.
    PackingRequiresStable,
    /// Packed layout contains storage with non-plain lifecycle behavior.
    PackingRequiresPlainStorage,
    /// A tag type was selected for a non-union declaration.
    TagRequiresUnion,
    /// C-compatible union layout omitted its explicit tag type.
    CUnionRequiresTag,
    /// An opaque size was supplied for a declaration with a body.
    OpaqueSizeRequiresBodylessStruct,
    /// A bodyless struct did not provide a complete opaque layout contract.
    BodylessStructRequiresSizeAndAlignment,
    /// Opaque storage selected a layout mode without a stable ABI contract.
    OpaqueStorageRequiresStableOrC,
    /// The opaque size is not a multiple of its required alignment.
    OpaqueSizeNotAligned {
        /// Exact requested byte size.
        size: u64,
        /// Required byte alignment.
        alignment: u64,
    },
    /// An unrepresented union tag was selected outside C union layout.
    TaglessUnionRequiresCLayout,
}

impl DiagnosticLayoutProblem {
    /// Returns the stable problem category key.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::MissingMode => "missing_mode",
            Self::UnsupportedMode(_) => "unsupported_mode",
            Self::DuplicateOption(_) => "duplicate_option",
            Self::UnknownOption(_) => "unknown_option",
            Self::UnexpectedPositionalArgument => "unexpected_positional_argument",
            Self::OptionNotConstant(_) => "option_not_constant",
            Self::OptionNotPowerOfTwo { .. } => "option_not_power_of_two",
            Self::TransparentUnion => "transparent_union",
            Self::TransparentFieldCount { .. } => "transparent_field_count",
            Self::TransparentOption(_) => "transparent_option",
            Self::PackingRequiresStable => "packing_requires_stable",
            Self::PackingRequiresPlainStorage => "packing_requires_plain_storage",
            Self::TagRequiresUnion => "tag_requires_union",
            Self::CUnionRequiresTag => "c_union_requires_tag",
            Self::OpaqueSizeRequiresBodylessStruct => "opaque_size_requires_bodyless_struct",
            Self::BodylessStructRequiresSizeAndAlignment => {
                "bodyless_struct_requires_size_and_alignment"
            }
            Self::OpaqueStorageRequiresStableOrC => "opaque_storage_requires_stable_or_c",
            Self::OpaqueSizeNotAligned { .. } => "opaque_size_not_aligned",
            Self::TaglessUnionRequiresCLayout => "tagless_union_requires_c_layout",
        }
    }
}

/// Exact reason a declared union-tag contract is invalid.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticUnionTagProblem {
    /// The selected tag-type spelling is not a supported integer type.
    UnsupportedType(String),
    /// A variant tag was declared without an explicit union layout.
    RequiresExplicitLayout,
    /// A tag directive did not contain exactly one argument.
    ArgumentCount {
        /// Number of supplied arguments.
        actual: u64,
    },
    /// The tag expression did not evaluate to an integer constant.
    ValueNotConstant,
    /// Another variant already uses this tag value.
    DuplicateValue,
    /// The tag value cannot be represented by the selected tag type.
    ValueOutsideSelectedType {
        /// Whether the selected tag type is signed.
        signed: bool,
        /// Selected tag-type width in bits.
        width_bits: u16,
        /// Whether the rejected value is negative.
        value_negative: bool,
        /// Significant magnitude bits required by the rejected value.
        value_bits: u64,
    },
    /// Only some union variants declared explicit tag values.
    PartialExplicitTags {
        /// Number of variants with explicit tags.
        explicit: u64,
        /// Total number of variants.
        total: u64,
    },
    /// Inferred tags included a negative value and cannot select an unsigned storage type.
    NegativeInferredValue,
    /// A tagless union variant also supplied a represented tag value.
    TaglessUnionHasVariantTag,
    /// Inferred tags require more than the maximum supported integer width.
    InferredValueTooWide {
        /// Significant bits required by the largest value.
        actual_bits: u64,
        /// Maximum supported significant bits.
        maximum_bits: u64,
    },
}

impl DiagnosticUnionTagProblem {
    /// Returns the stable problem category key.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::UnsupportedType(_) => "unsupported_type",
            Self::RequiresExplicitLayout => "requires_explicit_layout",
            Self::ArgumentCount { .. } => "argument_count",
            Self::ValueNotConstant => "value_not_constant",
            Self::DuplicateValue => "duplicate_value",
            Self::ValueOutsideSelectedType { .. } => "value_outside_selected_type",
            Self::PartialExplicitTags { .. } => "partial_explicit_tags",
            Self::NegativeInferredValue => "negative_inferred_value",
            Self::TaglessUnionHasVariantTag => "tagless_union_has_variant_tag",
            Self::InferredValueTooWide { .. } => "inferred_value_too_wide",
        }
    }
}

/// Exact reason a declared copy contract is invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCopyContractProblem {
    /// The type declares lifecycle behavior that precludes implicit copying.
    LifecycleBehavior,
    /// The zero-argument copy directive received arguments.
    UnexpectedArguments {
        /// Number of supplied arguments.
        actual: u64,
    },
    /// Conditional member copyability requires a generic containing type.
    ConditionalMembersRequireGenericType,
    /// One or more stored members do not provide a copy contract.
    NonCopyableMember,
}

impl DiagnosticCopyContractProblem {
    /// Returns the stable problem category key.
    pub const fn category(self) -> &'static str {
        match self {
            Self::LifecycleBehavior => "lifecycle_behavior",
            Self::UnexpectedArguments { .. } => "unexpected_arguments",
            Self::ConditionalMembersRequireGenericType => {
                "conditional_members_require_generic_type"
            }
            Self::NonCopyableMember => "non_copyable_member",
        }
    }
}

/// Exact source-level type category that cannot be stored inline.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStoredTypeProblem {
    /// A flexible array was used outside the final field of a C-layout product.
    FlexibleArrayRequiresFinalCStructField,
    /// A flexible array element lacks complete plain C storage.
    FlexibleArrayElementRequiresPlainCStorage,
    /// A dynamically sized slice was used as an inline member.
    Slice,
    /// A trait view was used as an inline member.
    TraitView,
    /// A suspended generator value was used as an inline member.
    Generator,
    /// Contextual `Self` remained unresolved at the storage boundary.
    ContextualSelf,
    /// A referenced named type has no finite inline representation.
    ReferencedTypeHasNoFiniteRepresentation,
}

impl DiagnosticStoredTypeProblem {
    /// Returns the stable problem category key.
    pub const fn category(self) -> &'static str {
        match self {
            Self::FlexibleArrayRequiresFinalCStructField => {
                "flexible_array_requires_final_c_struct_field"
            }
            Self::FlexibleArrayElementRequiresPlainCStorage => {
                "flexible_array_element_requires_plain_c_storage"
            }
            Self::Slice => "slice",
            Self::TraitView => "trait_view",
            Self::Generator => "generator",
            Self::ContextualSelf => "contextual_self",
            Self::ReferencedTypeHasNoFiniteRepresentation => {
                "referenced_type_has_no_finite_representation"
            }
        }
    }
}
