use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_task_resolution(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let values = self.context.semantic_values();
        let symbols = self.context.compiler_known_symbols();

        let completion = symbols
            .unary_representation_argument(values, RepresentationRole::Task, task.ty())
            .unwrap_or_else(|| {
                panic!(
                    "task type {:?} must provide its completion argument",
                    task.ty()
                )
            });

        let result = symbols
            .unary_representation_type(values, RepresentationRole::RunResult, completion)
            .map_err(SyntheticLoweringError::SemanticValue)?
            .unwrap_or_else(|| {
                panic!("completion type {completion:?} must have a run-result representation")
            });

        let representation = symbols
            .run_result_representation()
            .expect("compiler-known symbols must provide the run-result representation");

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
            .map_err(|cause| self.capacity_error(cause))?;

        let resolved = resolved
            .result()
            .expect("value-producing MIR operation must publish a result");

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, result)
            .map_err(|cause| self.capacity_error(cause))?;

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

        outcome
            .resolve(
                builder,
                block,
                source,
                [
                    MirOperationKind::Finalize(place.clone()),
                    MirOperationKind::Destroy(place),
                ],
            )
            .map_err(|cause| self.capacity_error(cause))
    }
}
