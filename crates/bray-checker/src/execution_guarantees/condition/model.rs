use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueData;
use std::sync::Arc;

/// A bounded, source-independent value used to compare entry and completion conditions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionCondition {
    /// A checked Boolean value.
    Boolean(bool),
    /// A checked literal value.
    Literal(Arc<ConstantValueData>),
    /// The value of a supplied input before execution begins.
    Input(crate::execution_guarantees::ExecutionPlace),
    /// The value produced by normal completion.
    Result,
    /// The value produced by one runtime expression, without inventing its contents.
    Expression(bray_bound_tree::BoundExpressionId),
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
    /// A retained receiver observed after one call completes normally.
    PostState(
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundReferenceTarget,
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
                | Self::Entry {
                    condition: value, ..
                } => condition = value,
                _ => return false,
            }
        }
    }
}
