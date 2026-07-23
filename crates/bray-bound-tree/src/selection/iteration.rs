use bray_symbols::{
    CallableDefinitionId, ImplementationInstanceId, ImplementationRequirementKey, TypeId,
};

use crate::{BoundExpressionId, BoundIterationSource, IterationSourceMode};

/// The checked types participating in one selected iteration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedIterationTypes {
    source: TypeId,
    cursor: TypeId,
    element: TypeId,
}

impl SelectedIterationTypes {
    /// Creates one source, cursor, and element type relationship.
    pub const fn new(source: TypeId, cursor: TypeId, element: TypeId) -> Self {
        Self {
            source,
            cursor,
            element,
        }
    }

    /// Returns the checked source type.
    pub const fn source(self) -> TypeId {
        self.source
    }

    /// Returns the selected cursor type.
    pub const fn cursor(self) -> TypeId {
        self.cursor
    }

    /// Returns the selected element type.
    pub const fn element(self) -> TypeId {
        self.element
    }
}

/// One implementation witness and callable operation selected for an iteration protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedIterationProtocolOperation {
    requirement: ImplementationRequirementKey,
    witness: ImplementationInstanceId,
    callable: CallableDefinitionId,
}

impl SelectedIterationProtocolOperation {
    /// Creates one selected protocol implementation and operation.
    pub const fn new(
        requirement: ImplementationRequirementKey,
        witness: ImplementationInstanceId,
        callable: CallableDefinitionId,
    ) -> Self {
        Self {
            requirement,
            witness,
            callable,
        }
    }

    /// Returns the exact implementation requirement.
    pub const fn requirement(&self) -> ImplementationRequirementKey {
        self.requirement
    }

    /// Returns the exact selected implementation witness.
    pub const fn witness(&self) -> ImplementationInstanceId {
        self.witness
    }

    /// Returns the callable operation supplied by the implementation.
    pub const fn callable(&self) -> CallableDefinitionId {
        self.callable
    }
}

/// The exact protocols and operations selected for one iteration source occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedIterationSource {
    expression: BoundExpressionId,
    source: BoundIterationSource,
    types: SelectedIterationTypes,
    iterable: SelectedIterationProtocolOperation,
    iterator: SelectedIterationProtocolOperation,
}

impl SelectedIterationSource {
    /// Creates one complete selected iteration source.
    pub const fn new(
        expression: BoundExpressionId,
        source: BoundIterationSource,
        types: SelectedIterationTypes,
        iterable: SelectedIterationProtocolOperation,
        iterator: SelectedIterationProtocolOperation,
    ) -> Self {
        Self {
            expression,
            source,
            types,
            iterable,
            iterator,
        }
    }

    /// Returns the iteration expression owning this selection.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the iteration source expression.
    pub const fn source(&self) -> BoundExpressionId {
        self.source.expression()
    }

    /// Returns how the source is accessed.
    pub const fn mode(&self) -> IterationSourceMode {
        self.source.mode()
    }

    /// Returns the checked source type.
    pub const fn source_type(&self) -> TypeId {
        self.types.source()
    }

    /// Returns the selected cursor type.
    pub const fn cursor_type(&self) -> TypeId {
        self.types.cursor()
    }

    /// Returns the selected element type.
    pub const fn element_type(&self) -> TypeId {
        self.types.element()
    }

    /// Returns the selected `Iterable` requirement.
    pub const fn iterable_requirement(&self) -> ImplementationRequirementKey {
        self.iterable.requirement()
    }

    /// Returns the exact `Iterable` implementation witness.
    pub const fn iterable_witness(&self) -> ImplementationInstanceId {
        self.iterable.witness()
    }

    /// Returns the selected `Iterator` requirement.
    pub const fn iterator_requirement(&self) -> ImplementationRequirementKey {
        self.iterator.requirement()
    }

    /// Returns the exact `Iterator` implementation witness.
    pub const fn iterator_witness(&self) -> ImplementationInstanceId {
        self.iterator.witness()
    }

    /// Returns the selected source-to-cursor operation.
    pub const fn iterate(&self) -> CallableDefinitionId {
        self.iterable.callable()
    }

    /// Returns the selected cursor-advance operation.
    pub const fn next(&self) -> CallableDefinitionId {
        self.iterator.callable()
    }
}
