use bray_symbols::TypeId;

use crate::{
    BoundCallResult, BoundCallableTarget, BoundExpressionId, BoundResolvedCall, BoundUnitId,
};

/// The checked access mode used to create an iteration subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IterationAccessMode {
    /// Create and consume a shared borrow of the source value.
    Shared,
    /// Create and consume a mutable borrow of the source value.
    Mutable,
    /// Move and consume the source value itself.
    Consuming,
}

/// An invalid selected iteration protocol contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckedIterationProtocolError {
    /// The selected `iterate` operation is not a declared immediate call.
    InvalidIterateOperation,
    /// The selected `next` operation is not a declared immediate call.
    InvalidNextOperation,
    /// The selected `iterate` operation does not produce the checked cursor type.
    CursorTypeMismatch,
    /// The selected `next` operation does not produce the checked nullable-element type.
    NextResultTypeMismatch,
}

/// Exact selected operations and types for lowering one `for` iteration protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedForIterationProtocol {
    access_mode: IterationAccessMode,
    element_type: TypeId,
    next_result_type: TypeId,
    cursor_type: TypeId,
    iterate: BoundResolvedCall,
    next: BoundResolvedCall,
}

impl CheckedForIterationProtocol {
    /// Creates a selected protocol after validating both hidden calls.
    pub fn try_new(
        access_mode: IterationAccessMode,
        element_type: TypeId,
        next_result_type: TypeId,
        cursor_type: TypeId,
        iterate: BoundResolvedCall,
        next: BoundResolvedCall,
    ) -> Result<Self, CheckedIterationProtocolError> {
        validate_declared_immediate_call(
            &iterate,
            CheckedIterationProtocolError::InvalidIterateOperation,
        )?;

        validate_declared_immediate_call(
            &next,
            CheckedIterationProtocolError::InvalidNextOperation,
        )?;

        if iterate.result() != BoundCallResult::Immediate(cursor_type) {
            return Err(CheckedIterationProtocolError::CursorTypeMismatch);
        }

        if next.result() != BoundCallResult::Immediate(next_result_type) {
            return Err(CheckedIterationProtocolError::NextResultTypeMismatch);
        }

        Ok(Self {
            access_mode,
            element_type,
            next_result_type,
            cursor_type,
            iterate,
            next,
        })
    }

    /// Returns how the source expression is accessed.
    pub const fn access_mode(&self) -> IterationAccessMode {
        self.access_mode
    }

    /// Returns the exact checked element type.
    pub const fn element_type(&self) -> TypeId {
        self.element_type
    }

    /// Returns the checked nullable-element result type of `next`.
    pub const fn next_result_type(&self) -> TypeId {
        self.next_result_type
    }

    /// Returns the exact owned cursor type.
    pub const fn cursor_type(&self) -> TypeId {
        self.cursor_type
    }

    /// Returns the selected one-time `iterate` call.
    pub const fn iterate(&self) -> &BoundResolvedCall {
        &self.iterate
    }

    /// Returns the selected repeated `next` call.
    pub const fn next(&self) -> &BoundResolvedCall {
        &self.next
    }
}

fn validate_declared_immediate_call(
    call: &BoundResolvedCall,
    error: CheckedIterationProtocolError,
) -> Result<(), CheckedIterationProtocolError> {
    if !matches!(call.target(), BoundCallableTarget::Declaration(_))
        || !matches!(call.result(), BoundCallResult::Immediate(_))
    {
        return Err(error);
    }

    Ok(())
}

/// The selected or recovered protocol decision for one `for` expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckedForIterationResolution {
    /// Both compiler-known protocol operations were selected exactly.
    Selected(Box<CheckedForIterationProtocol>),
    /// Earlier recovery prevented sound protocol selection.
    Recovered,
}

/// Durable iteration facts for one exact `for` expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedForIterationFacts {
    expression: BoundExpressionId,
    resolution: CheckedForIterationResolution,
}

impl CheckedForIterationFacts {
    /// Creates facts for a fully selected `for` protocol.
    pub fn selected(expression: BoundExpressionId, protocol: CheckedForIterationProtocol) -> Self {
        Self {
            expression,
            resolution: CheckedForIterationResolution::Selected(Box::new(protocol)),
        }
    }

    /// Creates an explicit recovery fact without selecting fallback operations.
    pub const fn recovered(expression: BoundExpressionId) -> Self {
        Self {
            expression,
            resolution: CheckedForIterationResolution::Recovered,
        }
    }

    /// Returns the exact `for` expression described by this fact.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the selected protocol or explicit recovery state.
    pub const fn resolution(&self) -> &CheckedForIterationResolution {
        &self.resolution
    }

    pub(super) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.expression.unit() == unit
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, FunctionSymbolId, GenericArgument,
        GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData, SemanticValueStore,
        SymbolId, TypeData,
    };

    use super::{CheckedForIterationProtocol, CheckedIterationProtocolError, IterationAccessMode};
    use crate::{BoundCallResult, BoundCallableTarget, BoundResolvedCall};

    #[test]
    fn selected_protocol_retains_exact_calls_types_and_access_mode() {
        let values = crate::test_support::semantic_values();
        let element_type = crate::test_support::error_type_in(&values);
        let cursor_type = match values.intern_type(TypeData::Slice(element_type)) {
            Ok(ty) => ty,
            Err(error) => panic!("test cursor type must be interned: {error:?}"),
        };
        let next_result_type = match values.intern_type(TypeData::Nullable(element_type)) {
            Ok(ty) => ty,
            Err(error) => panic!("test nullable element type must be interned: {error:?}"),
        };

        let iterate = resolved_call(&values, 10, cursor_type);
        let next = resolved_call(&values, 11, next_result_type);

        let protocol = match CheckedForIterationProtocol::try_new(
            IterationAccessMode::Mutable,
            element_type,
            next_result_type,
            cursor_type,
            iterate,
            next,
        ) {
            Ok(protocol) => protocol,
            Err(error) => panic!("selected test protocol must validate: {error:?}"),
        };

        assert_eq!(protocol.access_mode(), IterationAccessMode::Mutable);
        assert_eq!(protocol.element_type(), element_type);
        assert_eq!(protocol.next_result_type(), next_result_type);
        assert_eq!(protocol.cursor_type(), cursor_type);

        assert!(matches!(
            protocol.iterate().target(),
            BoundCallableTarget::Declaration(_)
        ));

        assert!(matches!(
            protocol.next().target(),
            BoundCallableTarget::Declaration(_)
        ));
    }

    #[test]
    fn selected_protocol_rejects_an_incorrect_iterate_result() {
        let values = crate::test_support::semantic_values();
        let element_type = crate::test_support::error_type_in(&values);
        let cursor_type = match values.intern_type(TypeData::Slice(element_type)) {
            Ok(ty) => ty,
            Err(error) => panic!("test cursor type must be interned: {error:?}"),
        };
        let next_result_type = match values.intern_type(TypeData::Nullable(element_type)) {
            Ok(ty) => ty,
            Err(error) => panic!("test nullable element type must be interned: {error:?}"),
        };

        let result = CheckedForIterationProtocol::try_new(
            IterationAccessMode::Shared,
            element_type,
            next_result_type,
            cursor_type,
            resolved_call(&values, 12, element_type),
            resolved_call(&values, 13, next_result_type),
        );

        assert_eq!(
            result,
            Err(CheckedIterationProtocolError::CursorTypeMismatch)
        );
    }

    fn resolved_call(
        values: &SemanticValueStore,
        symbol: u32,
        result: bray_symbols::TypeId,
    ) -> BoundResolvedCall {
        let definition = FunctionSymbolId::from_symbol_id(SymbolId::new(symbol));
        let callable = callable_instance(values, definition);

        BoundResolvedCall::new(
            BoundCallableTarget::Declaration(callable),
            [],
            BoundCallResult::Immediate(result),
        )
    }

    fn callable_instance(
        values: &SemanticValueStore,
        definition: FunctionSymbolId,
    ) -> CallableInstanceData {
        let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
            panic!("function must be a generic owner");
        };

        let substitution = match GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        ) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty callable substitution must validate: {error:?}"),
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
