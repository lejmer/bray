use bray_compiler_known::RepresentationRole;
use bray_ir::{MirOperationKind, MirPlace, MirSourceAnchor, MirUnitBuilder};
use bray_symbols::{CallableExecution, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<(), C::Error> {
        builder
            .push_operation(block, source.clone(), operation, None)
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(())
    }

    pub(super) fn push_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let outcome = self.cleanup_outcome(builder, block, source)?;

        self.push_selected_call(
            builder,
            block,
            source,
            place,
            callable,
            bray_bound_tree::BoundCallResult::Immediate(callable.result),
        )?;

        let completed = outcome
            .check(builder, block, source)
            .map_err(|cause| self.mir_error(source, cause))?;

        self.finish_cleanup_outcome(builder, completed, source, &outcome)
    }

    pub(super) fn push_static_finalizer_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirValueId), C::Error> {
        let result = match callable.execution {
            CallableExecution::Synchronous => {
                bray_bound_tree::BoundCallResult::Immediate(callable.result)
            }
            CallableExecution::Asynchronous => {
                let future = self
                    .context
                    .compiler_known_symbols()
                    .unary_representation_type(
                        self.context.semantic_values(),
                        RepresentationRole::Future,
                        callable.result,
                    )
                    .map_err(SyntheticLoweringError::SemanticValue)?
                    .ok_or_else(|| SyntheticLoweringError::MissingRepresentation {
                        role: RepresentationRole::Future,
                        argument: Some(callable.result),
                    })?;

                bray_bound_tree::BoundCallResult::LazyFuture(
                    bray_bound_tree::BoundFutureConstruction::new(callable.result, future),
                )
            }
        };

        let value = self.push_selected_call(builder, block, source, place, callable, result)?;

        if callable.execution == CallableExecution::Synchronous {
            self.check_lifecycle_value(builder, block, source, value, callable.result)
        } else {
            Ok((block, value))
        }
    }

    pub(super) fn push_selected_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
        result: bray_bound_tree::BoundCallResult,
    ) -> Result<bray_ir::MirValueId, C::Error> {
        let receiver = self
            .context
            .semantic_values()
            .type_data(callable.receiver)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let borrow = match receiver.as_ref() {
            TypeData::Borrow { kind, .. } => Some(*kind),
            _ => None,
        };

        crate::lifecycle_call::lower_lifecycle_call(
            bray_ir::MirCallableReference::new(callable.callable, callable.abi),
            callable.receiver,
            place,
            borrow,
            result,
            true,
            |operation, result| builder.push_operation(block, source.clone(), operation, result),
        )
        .map_err(|cause| match cause {
            bray_ir::MirUnitBuildError::MissingOperationResult(operation) => {
                SyntheticLoweringError::MissingOperationResult {
                    source: source.clone(),
                    operation,
                }
                .into()
            }
            cause => self.mir_error(source, cause),
        })
    }
}
