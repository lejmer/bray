mod effect;
pub use effect::{DependencyEffectOutcomes, DependencyMutationKind, DependencyStorageEffect};
mod application;
mod captured;
mod constant;
mod contextual_self;
mod dependency;
mod error;
mod id;
mod numeric;
mod ownership;
mod preservation;
mod store;
mod substitution;
mod substitution_apply;
mod ty;
mod witness;

pub use application::{
    CallableDefinitionId, CallableInstanceData, ImplementationInstanceData, PredicateInstanceData,
    TraitApplicationData,
};
pub use captured::{CapturedCallable, CapturedDeferredInvocation, CapturedFailureOutcome, CapturedValueOutcome, CapturedValueOperation, CapturedValueTemplate, DependencyFailureKind};
pub use constant::{
    AnyConstantDefinitionId, ConstantBinaryOperation, ConstantField, ConstantProjection,
    ConstantProjectionKind, ConstantTermData, ConstantUnaryOperation, ConstantValueData,
    ConstantValueKind, IntegerConstant, IntegerSign, RealConstantBits, TargetSizedIntegerType,
};
pub use dependency::{
    DependencyBorrowRelation, DependencyCallInput, DependencyCallOutput,
    DependencyContractTemplateData, DependencyGuard, DependencyProjection, DependencyReferenceKind,
    DependencyReferenceProvenance, DependencyRequirement, DependencyRequirementKind,
    DependencyStoredValue, DependencyStoredValuePath, DependencySubject, DependencySubjectRoot,
    GuardedDependencyRequirement, LifecycleObligationKind,
};
pub use witness::{DependencyWitnessCarrier, DependencyWitnessSupport};
pub use error::{SemanticValueStoreCreateError, SemanticValueStoreError};
pub use id::{
    CallableInstanceId, ConstantTermId, ConstantValueId, DependencyContractTemplateId,
    GenericSubstitutionId, ImplementationInstanceId, SemanticValueKind, SemanticValueStoreId,
    StaticReferenceId, TraitApplicationId, TypeId,
};
pub use store::SemanticValueStore;
pub use substitution::{
    GenericArgument, GenericArgumentKind, GenericBinding, GenericOwnerId, GenericSubstitutionData,
    GenericSubstitutionShapeError,
};
pub use ty::{
    BorrowKind, CallableAbi, CallableConstness, CallableDependencyContracts, CallableExecution,
    CallableParameterData, CallableParameterMode, CallableParameterName, CallablePosition,
    CallableTrust, CallableTypeData, SelfTypeContext, TypeData,
};

pub use numeric::NumericTruncation;

pub use ownership::{DependencyOwnedResult, DependencyOwnedTransfer};
pub use preservation::DependencyValuePreservation;
