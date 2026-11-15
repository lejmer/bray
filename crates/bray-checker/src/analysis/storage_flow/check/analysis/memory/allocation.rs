use bray_bound_tree::{CheckedExpressionTypes, CheckedMemoryOperation, CheckedMemoryOperationKind, CheckedMemoryOperations, StorageProjection};
use bray_symbols::{DependencyProjection, NamedTypeSymbolId, TypeData};

use crate::{CheckerRequestContext, CheckerUnitView};
use crate::analysis::storage_flow::memory::{Source, Target};
use crate::analysis::storage_flow::model::{StorageFlowInput, StorageFlowState};
use super::super::core::StorageFlowCollector;

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_allocation_pointer<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        operations: &CheckedMemoryOperations,
        types: &CheckedExpressionTypes,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let Some(operation) = operations.operations().iter().find(|operation| matches!(
            operation.kind(), CheckedMemoryOperationKind::Allocate | CheckedMemoryOperationKind::Deallocate,
        )) else {
            return Ok(());
        };

        let expression = match operation.kind() {
            CheckedMemoryOperationKind::Allocate => operation.expression(),
            CheckedMemoryOperationKind::Deallocate => operation.arguments()[0],
            _ => unreachable!("selected allocation operation"),
        };

        let ty = request.semantic_values().unborrowed_type(types.expression(expression)
            .expect("checked allocation operation retains its record type").ty());

        let TypeData::Named { definition: NamedTypeSymbolId::Struct(structure), .. } =
            *request.semantic_values().type_data(ty)
        else {
            panic!("recognized allocation operation requires its ordinary record: {ty:?}");
        };

        let pointer = crate::representation::struct_field(request.context(), structure, "pointer")?;
        let pointer_type = crate::storage::projected_value_type(request, ty, DependencyProjection::ProductField(pointer))?;

        self.allocation_pointer = Some((pointer, pointer_type.value()
            .expect("recognized allocation pointer has its checked type")));

        Ok(())
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn deallocation_pointer(
        &self,
        state: &StorageFlowState,
        operation: &CheckedMemoryOperation,
    ) -> Option<Target> {
        let pointer = self.memory_argument_target(state, operation, operation.arguments()[0])?;

        Some(if matches!(operation.kind(), CheckedMemoryOperationKind::Deallocate) {
            let (field, _) = self.input.allocation_pointer.expect("allocation field is prepared");

            pointer.project(&[StorageProjection::ProductField(field)])
        } else {
            pointer
        })
    }

    pub(super) fn deallocation_source(
        &self,
        state: &StorageFlowState,
        operation: &CheckedMemoryOperation,
    ) -> Option<Source> {
        if matches!(operation.kind(), CheckedMemoryOperationKind::Deallocate) {
            self.deallocation_pointer(state, operation).map(Source::Value)
        } else {
            self.memory_argument_reference(state, operation, operation.arguments()[0])
        }
    }
}
