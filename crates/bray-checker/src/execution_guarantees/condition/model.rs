use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueData;
use std::sync::Arc;

/// How a current observation relates to the storage or value that retains it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionObservationKind {
    /// Current contents belong to this complete owned value.
    Owned,
    /// Current reached data is observed through a retained reference value.
    Reference,
    /// Copied scalar data is observed at an independently identified storage place.
    Scalar,
}

/// A bounded, source-independent value used to compare entry and completion conditions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionCondition {
    /// A checked Boolean value.
    Boolean(bool),
    /// An active union tag in a checked aggregate snapshot.
    Variant(bray_symbols::UnionVariantSymbolId),
    /// A constructed union snapshot with its exact active tag.
    Tagged(bray_symbols::UnionVariantSymbolId, Arc<ExecutionCondition>),
    /// A test of a union tag, independent of its payload.
    VariantTest(bray_symbols::UnionVariantSymbolId, Arc<ExecutionCondition>),
    /// A checked literal value.
    Literal(Arc<ConstantValueData>),
    /// The value of a supplied input before execution begins.
    Input(crate::execution_guarantees::ExecutionPlace),
    /// The value produced by normal completion.
    Result,
    /// The value produced by one runtime expression, without inventing its contents.
    Expression(bray_bound_tree::BoundExpressionId),
    /// An input captured by one deferred producer in the actual future owner.
    CapturedInput {
        owner: Arc<ExecutionCondition>,
        invocation: bray_bound_tree::BoundExecutionSite,
        ordinal: bray_symbols::SymbolOrdinal,
    },
    /// The capability produced by successful entry at one actual with occurrence.
    ScopedCapability(bray_bound_tree::BoundExpressionId),
    /// The present branch of a nullable value, retaining its contained owner.
    Present(Arc<ExecutionCondition>),
    /// An aggregate snapshot with checked component values over its immutable base.
    /// Unlisted components retain the base identity; explicit unknown components do not.
    Constructed(
        Arc<ExecutionCondition>,
        Arc<[(bray_bound_tree::StorageProjection, ExecutionCondition)]>,
    ),
    /// Current data retains its identity independently of its observation role.
    /// Reference contents never transfer ownership of the reached storage.
    Observation {
        owner: Arc<ExecutionCondition>,
        contents: Arc<ExecutionCondition>,
        kind: ExecutionObservationKind,
    },
    /// Fresh contents observed after a checked mutation. This establishes no guarantee.
    PostState(
        bray_bound_tree::StorageMutation,
        crate::execution_guarantees::ExecutionPlace,
    ),
    /// The current value of storage where control-flow predecessors meet.
    Joined(
        bray_bound_tree::BoundUnitId,
        u32,
        crate::execution_guarantees::ExecutionPlace,
    ),
    /// A selected built-in operation on other known values.
    Operation(BoundOperator, Arc<[ExecutionCondition]>),
    /// A checked predicate application, compared by declaration and selected arguments.
    Predicate(
        bray_symbols::PredicateDefinitionSymbolId,
        bray_symbols::GenericSubstitutionId,
        Arc<[ExecutionCondition]>,
    ),
    /// A checked deterministic callable application with stable declaration and argument identity.
    Call(
        bray_symbols::CallableInstanceData,
        Option<bray_symbols::ImplementationInstanceData>,
        Arc<[bray_symbols::ImplementationInstanceData]>,
        Arc<[ExecutionCondition]>,
    ),
    /// One component of an observed immutable value.
    Projection(
        bray_symbols::ConstantProjectionKind,
        Arc<ExecutionCondition>,
    ),
    /// A predicate occurrence that must be established by trusted evidence.
    Trusted(Arc<ExecutionCondition>),
    /// The contents reached through a borrowed predicate argument, rather than an immutable value snapshot.
    Borrowed(Arc<ExecutionCondition>),
    /// A conditional guarantee's entry guard; captured guards have already been substituted at entry.
    Entry {
        condition: Arc<ExecutionCondition>,
        captured: bool,
    },
    /// Meaning that cannot currently be established.
    Unknown,
}

impl ExecutionCondition {
    // Bound recursive normalization and implication work, including adversarial source depth.
    pub(crate) const WORK_LIMIT: usize = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

    pub(super) fn is_unknown(&self) -> bool {
        let mut condition = self;

        loop {
            match condition {
                Self::Unknown => return true,
                Self::Trusted(value)
                | Self::Borrowed(value)
                | Self::Present(value)
                | Self::Projection(_, value)
                | Self::VariantTest(_, value)
                | Self::Entry {
                    condition: value, ..
                }
                | Self::CapturedInput { owner: value, .. } => condition = value,
                _ => return false,
            }
        }
    }
    pub(crate) fn contains_trusted(&self) -> bool {
        match self {
            Self::Trusted(_) => true,
            Self::Operation(_, operands)
            | Self::Predicate(_, _, operands)
            | Self::Call(_, _, _, operands) => operands.iter().any(Self::contains_trusted),
            Self::Tagged(_, value)
            | Self::VariantTest(_, value)
            | Self::Present(value)
            | Self::Projection(_, value)
            | Self::Borrowed(value)
            | Self::Entry {
                condition: value, ..
            }
            | Self::CapturedInput { owner: value, .. } => value.contains_trusted(),
            Self::Observation { owner, contents: value, .. } => owner.contains_trusted() || value.contains_trusted(),
            Self::Constructed(base, fields) => {
                base.contains_trusted() || fields.iter().any(|(_, value)| value.contains_trusted())
            }
            Self::Boolean(_)
            | Self::Variant(_)
            | Self::Literal(_)
            | Self::Input(_)
            | Self::Result
            | Self::Expression(_)
            | Self::ScopedCapability(_)
            | Self::PostState(_, _)
            | Self::Joined(_, _, _)
            | Self::Unknown => false,
        }
    }

    pub(super) fn contains_unknown(&self) -> bool {
        match self {
            Self::Operation(_, operands) => operands.iter().any(Self::contains_unknown),
            Self::Observation { owner, contents: value, .. } => owner.contains_unknown() || value.contains_unknown(),
            Self::Trusted(value)
            | Self::Borrowed(value)
            | Self::Present(value)
            | Self::Projection(_, value)
            | Self::VariantTest(_, value)
            | Self::Entry {
                condition: value, ..
            }
            | Self::CapturedInput { owner: value, .. } => value.contains_unknown(),
            _ => self.is_unknown(),
        }
    }
}
