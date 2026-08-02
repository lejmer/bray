use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnonymousCallableSymbolId, CallableDefinitionId, CallableInstanceData,
    ImplementationInstanceId, PredicateInstanceData, SymbolName, TypeId,
};

use crate::{BoundExpressionId, BoundNodeOrigin};

/// One explicit source generic argument awaiting candidate-specific binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundGenericArgument {
    syntax: SyntaxAnchor,
}

impl BoundGenericArgument {
    /// Creates a generic argument from its exact source syntax.
    pub const fn new(syntax: SyntaxAnchor) -> Self {
        Self { syntax }
    }

    /// Returns the exact generic-argument syntax anchor.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns whether parser recovery contributed to this argument.
    pub const fn is_recovered(self) -> bool {
        self.syntax.is_recovered()
    }
}

/// One source-ordered input to overload selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundArgument {
    expression: BoundExpressionId,
    name: Option<SymbolName>,
    is_recovered: bool,
}

impl BoundArgument {
    /// Creates a positional or named call argument.
    pub const fn new(
        expression: BoundExpressionId,
        name: Option<SymbolName>,
        is_recovered: bool,
    ) -> Self {
        Self {
            expression,
            name,
            is_recovered,
        }
    }

    /// Returns the argument expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the canonical argument name when named.
    pub const fn name(&self) -> Option<&SymbolName> {
        self.name.as_ref()
    }

    /// Returns whether recovery contributed to this argument.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// The exact callable value selected for an ordinary call expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundCallableTarget {
    /// A declaration and its complete generic substitution.
    ///
    /// Compiler-provided behavior is classified from this declaration identity, never from the
    /// callee's source spelling.
    Declaration(CallableInstanceData),
    /// A compile-time predicate and its complete generic substitution.
    Predicate(PredicateInstanceData),
    /// A separately bound anonymous callable unit.
    Anonymous(AnonymousCallableSymbolId),
    /// A dynamically selected callable value represented by its checked callable type.
    Indirect(TypeId),
}

impl BoundCallableTarget {
    /// Returns the declaration identity when the target is statically declared.
    pub const fn declaration(self) -> Option<CallableDefinitionId> {
        match self {
            Self::Declaration(instance) => Some(instance.definition()),
            Self::Predicate(_) | Self::Anonymous(_) | Self::Indirect(_) => None,
        }
    }
}

/// Checked facts for constructing one lazy async computation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundFutureConstruction {
    completion_type: TypeId,
    future_type: TypeId,
}

impl BoundFutureConstruction {
    /// Creates the result facts for invocation of an async callable.
    pub const fn new(completion_type: TypeId, future_type: TypeId) -> Self {
        Self {
            completion_type,
            future_type,
        }
    }

    /// Returns the result type declared by the async callable.
    pub const fn completion_type(self) -> TypeId {
        self.completion_type
    }

    /// Returns the produced compiler-known `Future<T>` type.
    pub const fn future_type(self) -> TypeId {
        self.future_type
    }
}

/// Whether a selected call executes immediately or constructs a lazy future.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundCallResult {
    /// Immediate execution producing an ordinary value.
    Immediate(TypeId),
    /// Lazy async-frame construction producing `Future<T>`.
    LazyFuture(BoundFutureConstruction),
}

impl BoundCallResult {
    /// Returns the source-visible type produced by the call expression.
    pub const fn ty(self) -> TypeId {
        match self {
            Self::Immediate(ty) => ty,
            Self::LazyFuture(construction) => construction.future_type(),
        }
    }
}

/// The complete callable selection needed to interpret a checked call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundResolvedCall {
    target: BoundCallableTarget,
    implementation_witnesses: Arc<[ImplementationInstanceId]>,
    trait_dispatch: Option<bray_symbols::TraitConstraintDispatch>,
    result: BoundCallResult,
}

impl BoundResolvedCall {
    /// Creates a resolved call with witnesses in canonical semantic-set order.
    ///
    /// For a future-producing call, the target and witnesses identify the hidden async frame
    /// independently of the source-visible `Future<T>` type.
    pub fn new(
        target: BoundCallableTarget,
        implementation_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        result: BoundCallResult,
    ) -> Self {
        Self {
            target,
            implementation_witnesses: sorted_unique_shared_slice(implementation_witnesses),
            trait_dispatch: None,
            result,
        }
    }

    /// Returns this call with dispatch supplied by one surrounding generic constraint.
    pub const fn with_trait_dispatch(
        mut self,
        dispatch: bray_symbols::TraitConstraintDispatch,
    ) -> Self {
        self.trait_dispatch = Some(dispatch);

        self
    }

    /// Returns the exact declared, anonymous, or indirect callable target.
    pub const fn target(&self) -> BoundCallableTarget {
        self.target
    }

    /// Returns selected implementation witnesses in canonical semantic-set order.
    pub fn implementation_witnesses(&self) -> &[ImplementationInstanceId] {
        &self.implementation_witnesses
    }

    /// Returns the generic constraint supplying member dispatch.
    pub const fn trait_dispatch(&self) -> Option<bray_symbols::TraitConstraintDispatch> {
        self.trait_dispatch
    }

    /// Returns whether this call executes immediately or constructs a lazy future.
    pub const fn result(&self) -> BoundCallResult {
        self.result
    }
}

/// Whether ordinary call selection is still pending or has completed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundCallResolution {
    /// Overload selection and type checking have not completed yet.
    Pending,
    /// The exact callable target and invocation result are available.
    Resolved(BoundResolvedCall),
}

impl BoundCallResolution {
    /// Returns the selected call when semantic resolution has completed.
    pub const fn resolved(&self) -> Option<&BoundResolvedCall> {
        match self {
            Self::Pending => None,
            Self::Resolved(call) => Some(call),
        }
    }

    const fn ty(&self) -> Option<TypeId> {
        match self {
            Self::Pending => None,
            Self::Resolved(call) => Some(call.result().ty()),
        }
    }
}

/// An ordinary call expression retaining source inputs and semantic selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundCallExpression {
    origin: BoundNodeOrigin,
    callee: BoundExpressionId,
    generic_arguments: Arc<[BoundGenericArgument]>,
    arguments: Arc<[BoundArgument]>,
    operands: Arc<[BoundExpressionId]>,
    resolution: BoundCallResolution,
}

impl BoundCallExpression {
    /// Creates a call awaiting overload selection and type checking.
    pub fn pending(
        origin: BoundNodeOrigin,
        callee: BoundExpressionId,
        generic_arguments: impl IntoIterator<Item = BoundGenericArgument>,
        arguments: impl IntoIterator<Item = BoundArgument>,
    ) -> Self {
        Self::new(
            origin,
            callee,
            generic_arguments,
            arguments,
            BoundCallResolution::Pending,
        )
    }

    /// Creates a checked call with its exact semantic selection.
    pub fn resolved(
        origin: BoundNodeOrigin,
        callee: BoundExpressionId,
        generic_arguments: impl IntoIterator<Item = BoundGenericArgument>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        resolution: BoundResolvedCall,
    ) -> Self {
        Self::new(
            origin,
            callee,
            generic_arguments,
            arguments,
            BoundCallResolution::Resolved(resolution),
        )
    }

    fn new(
        origin: BoundNodeOrigin,
        callee: BoundExpressionId,
        generic_arguments: impl IntoIterator<Item = BoundGenericArgument>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        resolution: BoundCallResolution,
    ) -> Self {
        let generic_arguments = shared_slice(generic_arguments);
        let arguments = shared_slice(arguments);

        let operands = shared_slice(
            std::iter::once(callee).chain(arguments.iter().map(BoundArgument::expression)),
        );

        Self {
            origin,
            callee,
            generic_arguments,
            arguments,
            operands,
            resolution,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the callable expression supplied to overload selection.
    pub const fn callee(&self) -> BoundExpressionId {
        self.callee
    }

    /// Returns explicit generic arguments in source order.
    pub fn generic_arguments(&self) -> &[BoundGenericArgument] {
        &self.generic_arguments
    }

    /// Returns arguments in source evaluation order.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }

    /// Returns the pending or completed semantic resolution.
    pub const fn resolution(&self) -> &BoundCallResolution {
        &self.resolution
    }

    /// Returns the checked type once semantic resolution has completed.
    pub const fn ty(&self) -> Option<TypeId> {
        self.resolution.ty()
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        false
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, FunctionSymbolId, GenericArgument,
        GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData, SemanticValueStore,
        SymbolId, TypeData,
    };

    use super::{
        BoundCallExpression, BoundCallResult, BoundCallableTarget, BoundFutureConstruction,
        BoundGenericArgument, BoundResolvedCall,
    };
    use crate::{BoundExpressionId, BoundUnitId};

    #[test]
    fn resolved_async_calls_retain_lazy_future_and_exact_callable_facts() {
        let values = crate::test_support::semantic_values();
        let completion_type = crate::test_support::error_type_in(&values);

        let future_type = match values.intern_type(TypeData::Slice(completion_type)) {
            Ok(ty) => ty,
            Err(error) => panic!("future test type must be interned: {error:?}"),
        };

        let definition = FunctionSymbolId::from_symbol_id(SymbolId::new(7));
        let callable = callable_instance(&values, definition);

        let target = BoundCallableTarget::Declaration(callable);

        let resolved = BoundResolvedCall::new(
            target,
            [],
            BoundCallResult::LazyFuture(BoundFutureConstruction::new(completion_type, future_type)),
        );

        let unit = BoundUnitId::new(3);
        let callee = BoundExpressionId::from_slot(unit, 0);

        let generic_argument =
            BoundGenericArgument::new(crate::test_support::source_anchor().syntax());

        let expression = BoundCallExpression::resolved(
            crate::BoundNodeOrigin::source(crate::test_support::source_anchor()),
            callee,
            [generic_argument],
            [],
            resolved,
        );

        let Some(resolved) = expression.resolution().resolved() else {
            panic!("resolved call must retain its semantic selection");
        };

        assert_eq!(resolved.target().declaration(), Some(callable.definition()));

        assert_eq!(
            resolved
                .target()
                .declaration()
                .map(|target| target.symbol()),
            Some(definition.into())
        );

        assert_eq!(expression.ty(), Some(future_type));
        assert_eq!(expression.generic_arguments(), [generic_argument]);
        assert_eq!(expression.operands(), [callee]);

        let BoundCallResult::LazyFuture(construction) = resolved.result() else {
            panic!("async callable invocation must construct a lazy future");
        };

        assert_eq!(construction.completion_type(), completion_type);
        assert_eq!(construction.future_type(), future_type);

        let ordinary_future_return =
            BoundResolvedCall::new(target, [], BoundCallResult::Immediate(future_type));

        assert_eq!(
            ordinary_future_return.result(),
            BoundCallResult::Immediate(future_type)
        );
    }

    fn callable_instance(
        values: &SemanticValueStore,
        definition: FunctionSymbolId,
    ) -> CallableInstanceData {
        let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
            panic!("function must be a generic owner");
        };

        let substitution = GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        );

        let substitution = match substitution {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty callable substitution must be valid: {error:?}"),
        };

        let substitution = match values.intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty callable substitution must be interned: {error:?}"),
        };

        let Some(definition) = CallableDefinitionId::try_new(definition.into()) else {
            panic!("function must be a callable definition");
        };

        CallableInstanceData::new(definition, substitution)
    }
}
