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
            .map_err(|cause| self.capacity_error(cause))?;

        Ok(())
    }

    pub(super) fn push_owner_resolution(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let boolean = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let finalized = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Finalize(place.clone()),
                Some(boolean),
            )
            .map_err(|cause| self.capacity_error(cause))?
            .result()
            .expect("finalization publishes its disposition");

        let (completed, disposition, failed) = outcome
            .check_completion(
                builder,
                block,
                source,
                Some(bray_ir::MirOperand::Value(finalized)),
            )
            .map_err(|cause| self.capacity_error(cause))?;

        let destruction = builder
            .push_block(source.clone(), builder.block_kind(block))
            .map_err(|cause| self.capacity_error(cause))?;

        let argument = builder
            .push_block_parameter(destruction, source.clone(), boolean)
            .map_err(|cause| self.capacity_error(cause))?;

        builder.set_terminator(
            completed,
            source.clone(),
            bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(
                destruction,
                [disposition.expect("completed finalization retains its disposition")],
            )),
        );

        builder.set_terminator(
            failed,
            source.clone(),
            bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(
                destruction,
                [bray_ir::MirOperand::Immediate {
                    ty: boolean,
                    value: bray_ir::MirImmediateValue::Boolean(true),
                }],
            )),
        );

        self.push_lifecycle_operation(
            builder,
            destruction,
            source,
            MirOperationKind::Destroy {
                place,
                abandoned: Some(bray_ir::MirOperand::Value(argument)),
            },
        )?;

        let end = outcome
            .check(builder, destruction, source)
            .map_err(|cause| self.capacity_error(cause))?;

        self.finish_cleanup_outcome(builder, end, source, &outcome, &mut None)
    }

    pub(super) fn push_lifecycle_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirOperand), C::Error> {
        let outcome = self.cleanup_outcome(builder, block, source)?;

        let value = self.push_selected_call(
            builder,
            block,
            source,
            place,
            callable,
            bray_bound_tree::BoundCallResult::Immediate(callable.result),
        )?;

        let payloads = crate::operand::result_payloads(
            self.context.semantic_values(),
            self.context.compiler_known_symbols(),
            callable.result,
        );

        let (completed, abandoned) = outcome
            .retain_finalizer_result(
                builder,
                block,
                source,
                bray_ir::MirOperand::Value(value),
                (
                    bray_ir::MirCallableReference::new(callable.callable, callable.abi),
                    bray_bound_tree::BoundCallResult::Immediate(callable.result),
                ),
                payloads,
            )
            .map_err(|cause| self.capacity_error(cause))?;

        let mut abandoned = Some(abandoned);

        let completed =
            self.finish_cleanup_outcome(builder, completed, source, &outcome, &mut abandoned)?;

        Ok((
            completed,
            abandoned.expect("finalizer completion retains its disposition"),
        ))
    }

    pub(super) fn push_checked_lifecycle_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirValueId), C::Error> {
        let result = callable
            .invocation_result(
                self.context.semantic_values(),
                self.context.compiler_known_symbols(),
            )
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let value = self.push_selected_call(builder, block, source, place, callable, result)?;

        if callable.execution == CallableExecution::Synchronous {
            self.check_lifecycle_value(builder, block, source, value, callable.result)
        } else {
            Ok((block, value))
        }
    }

    pub(super) fn push_selected_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
        result: bray_bound_tree::BoundCallResult,
    ) -> Result<bray_ir::MirValueId, C::Error> {
        let receiver = self.context.semantic_values().type_data(callable.receiver);

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
            if callable.callable.definition().callable_symbol().kind()
                == bray_symbols::SymbolKind::Destructor
            {
                self.abandoned.clone()
            } else {
                None
            },
            |operation, result| builder.push_operation(block, source.clone(), operation, result),
        )
        .map_err(|cause| self.capacity_error(cause))
    }
}
