use crate::DiagnosticType;

/// User-visible purpose of an access to stored data.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStorageAccessPurpose {
    Read,
    Initialize,
    Write,
    Move,
    Copy,
    SharedBorrow,
    MutableBorrow,
    Assignment,
    MemberSelection,
    IndexSelection,
    SliceSelection,
    Projection,
}

impl DiagnosticStorageAccessPurpose {
    /// Returns the stable machine key for this access purpose.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Initialize => "initialize",
            Self::Write => "write",
            Self::Move => "move",
            Self::Copy => "copy",
            Self::SharedBorrow => "shared_borrow",
            Self::MutableBorrow => "mutable_borrow",
            Self::Assignment => "assignment",
            Self::MemberSelection => "member_selection",
            Self::IndexSelection => "index_selection",
            Self::SliceSelection => "slice_selection",
            Self::Projection => "projection",
        }
    }
}

/// Source-semantic root category for an access to stored data.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStorageRoot {
    Local,
    Parameter,
    Receiver,
    Static,
    AnonymousParameter,
    PredicateParameter,
    PostconditionResult,
    Result,
    Temporary,
    CustomIndexBorrow,
    IterationCursor,
    IterationElement,
    Allocation,
    CompilerCreated,
    Alternative,
    Borrow,
    BorrowedStorage,
    OwnedIndirection,
    Recovery,
}

impl DiagnosticStorageRoot {
    /// Returns the stable machine key for this storage-root category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Parameter => "parameter",
            Self::Receiver => "receiver",
            Self::Static => "static",
            Self::AnonymousParameter => "anonymous_parameter",
            Self::PredicateParameter => "predicate_parameter",
            Self::PostconditionResult => "postcondition_result",
            Self::Result => "result",
            Self::Temporary => "temporary",
            Self::CustomIndexBorrow => "custom_index_borrow",
            Self::IterationCursor => "iteration_cursor",
            Self::IterationElement => "iteration_element",
            Self::Allocation => "allocation",
            Self::CompilerCreated => "compiler_created",
            Self::Alternative => "alternative",
            Self::Borrow => "borrow",
            Self::BorrowedStorage => "borrowed_storage",
            Self::OwnedIndirection => "owned_indirection",
            Self::Recovery => "recovery",
        }
    }
}

/// One source-semantic step in an access path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStorageProjection {
    ProductField(String),
    TupleElement(u64),
    ElementFromStart(u64),
    ElementFromEnd(u64),
    ActiveUnionPayloadField { variant: String, field: String },
    IndexedElement,
    SliceRange { has_start: bool, has_end: bool },
    NullableValue,
    OwnedTarget,
}

impl DiagnosticStorageProjection {
    /// Returns the stable machine key for this projection category.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ProductField(_) => "product_field",
            Self::TupleElement(_) => "tuple_element",
            Self::ElementFromStart(_) => "element_from_start",
            Self::ElementFromEnd(_) => "element_from_end",
            Self::ActiveUnionPayloadField { .. } => "active_union_payload_field",
            Self::IndexedElement => "indexed_element",
            Self::SliceRange { .. } => "slice_range",
            Self::NullableValue => "nullable_value",
            Self::OwnedTarget => "owned_target",
        }
    }
}

/// Exact source-semantic access that failed storage-flow checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticStorageAccess {
    purpose: DiagnosticStorageAccessPurpose,
    root: DiagnosticStorageRoot,
    projections: Box<[DiagnosticStorageProjection]>,
    reached_type: DiagnosticType,
}

impl DiagnosticStorageAccess {
    /// Creates an access descriptor from validated checking data.
    pub fn new(
        purpose: DiagnosticStorageAccessPurpose,
        root: DiagnosticStorageRoot,
        projections: impl IntoIterator<Item = DiagnosticStorageProjection>,
        reached_type: DiagnosticType,
    ) -> Self {
        Self {
            purpose,
            root,
            projections: projections.into_iter().collect(),
            reached_type,
        }
    }

    /// Returns how the source operation uses the reached storage.
    pub const fn purpose(&self) -> DiagnosticStorageAccessPurpose {
        self.purpose
    }

    /// Returns the source-semantic category of the access root.
    pub const fn root(&self) -> DiagnosticStorageRoot {
        self.root
    }

    /// Returns the ordered source-semantic access path.
    pub fn projections(&self) -> &[DiagnosticStorageProjection] {
        &self.projections
    }

    /// Returns the type reached by the complete access path.
    pub const fn reached_type(&self) -> &DiagnosticType {
        &self.reached_type
    }
}
