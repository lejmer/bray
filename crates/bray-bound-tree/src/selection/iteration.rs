use bray_symbols::{
    CallableInstanceData, ConstantTermId, ImplementationInstanceId, ImplementationRequirementKey,
    TypeId,
};

use crate::{BoundExpressionId, BoundIterationSource, IterationSourceMode};

/// The checked types participating in one selected iteration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedIterationProtocolOperation {
    requirement: ImplementationRequirementKey,
    witness: ImplementationInstanceId,
    member: CallableInstanceData,
    fulfillment: CallableInstanceData,
}

impl SelectedIterationProtocolOperation {
    /// Creates one selected protocol implementation and operation.
    pub const fn new(
        requirement: ImplementationRequirementKey,
        witness: ImplementationInstanceId,
        member: CallableInstanceData,
        fulfillment: CallableInstanceData,
    ) -> Self {
        Self {
            requirement,
            witness,
            member,
            fulfillment,
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

    /// Returns the exact substituted protocol member selected for this operation.
    pub const fn member(&self) -> CallableInstanceData {
        self.member
    }

    /// Returns the exact substituted implementation callable that executes this operation.
    pub const fn fulfillment(&self) -> CallableInstanceData {
        self.fulfillment
    }
}

/// The exact protocols and operations selected for one iteration source occurrence.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedIterationSource {
    expression: BoundExpressionId,
    source: BoundIterationSource,
    types: SelectedIterationTypes,
    iterable: SelectedIterationProtocolOperation,
    iterator: SelectedIterationProtocolOperation,
    exact_count: Option<ConstantTermId>,
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
            exact_count: None,
        }
    }

    /// Returns this selection with its proven exact iteration count.
    pub const fn with_exact_count(mut self, exact_count: ConstantTermId) -> Self {
        self.exact_count = Some(exact_count);

        self
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

    /// Returns the exact iteration count when selected contracts and source facts prove it.
    pub const fn exact_count(&self) -> Option<ConstantTermId> {
        self.exact_count
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
    pub const fn iterate(&self) -> CallableInstanceData {
        self.iterable.fulfillment()
    }

    /// Returns the selected cursor-advance operation.
    pub const fn next(&self) -> CallableInstanceData {
        self.iterator.fulfillment()
    }

    /// Returns the selected source-to-cursor protocol member.
    pub const fn iterate_member(&self) -> CallableInstanceData {
        self.iterable.member()
    }

    /// Returns the selected cursor-advance protocol member.
    pub const fn next_member(&self) -> CallableInstanceData {
        self.iterator.member()
    }
}
