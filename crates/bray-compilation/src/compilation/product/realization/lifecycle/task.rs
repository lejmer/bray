use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::super::super::CodegenPreparationError;
use super::super::super::super::Compilation;
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation::product::realization) fn push_task_resolution(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<(), CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let symbols = self.available_compiler_known_symbols();

        let completion = symbols
            .unary_representation_argument(values, RepresentationRole::Task, task.ty())
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let result = symbols
            .unary_representation_type(values, RepresentationRole::RunResult, completion)
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let representation = symbols
            .run_result_representation()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let variants = bray_ir::MirRunResultVariants::new(
            representation.completed_variant(),
            representation.panicked_variant(),
            representation.cancelled_variant(),
        );

        let resolved = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::ResolveTask {
                    task: MirOperand::Move(task),
                    variants,
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::TaskResolution, runtime_abi),
                }),
                Some(result),
            )
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
            .result()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, result)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let place = MirPlace::new(storage, [], result);

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value: MirOperand::Value(resolved),
            },
        )?;

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Finalize(place.clone()),
        )?;

        self.push_lifecycle_operation(builder, block, source, MirOperationKind::Destroy(place))?;

        Ok(())
    }
}
