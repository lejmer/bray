use bray_ir::{MirOperationKind, MirPlace, MirSourceAnchor, MirUnitBuilder};
use bray_symbols::{CallableExecution, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext};

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
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let boolean = self
            .context
            .representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let action = self
            .context
            .lifecycle_action(place.ty(), bray_bound_tree::LifecyclePhase::Finalize)?;

        let (block, abandoned) = match action {
            bray_bound_tree::LifecycleAction::None => (
                block,
                bray_ir::MirOperand::Immediate {
                    ty: boolean,
                    value: bray_ir::MirImmediateValue::Boolean(false),
                },
            ),
            bray_bound_tree::LifecycleAction::Call(callable) => {
                // Invocation borrows this path while destruction retains the same owner.
                self.push_lifecycle_call(builder, block, source, place.clone(), callable, &outcome)?
            }
            action => {
                panic!("selected whole-value finalization must be an invocation, got {action:?}")
            }
        };

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Destroy {
                place,
                abandoned: Some(abandoned),
            },
        )?;

        let end = outcome
            .check(builder, block, source)
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
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirOperand), C::Error> {
        let value = self.push_selected_call(
            builder,
            block,
            source,
            place,
            callable,
            bray_bound_tree::BoundCallResult::Immediate(callable.result.completion_type()),
        )?;

        let payloads = crate::operand::result_payloads(
            self.context.semantic_values(),
            self.context.compiler_known_symbols(),
            callable.result.completion_type(),
        );

        let (completed, abandoned) = outcome
            .retain_finalizer_result(
                builder,
                block,
                source,
                bray_ir::MirOperand::Value(value),
                (
                    bray_ir::MirCallableReference::new(callable.callable, callable.abi),
                    bray_bound_tree::BoundCallResult::Immediate(callable.result.completion_type()),
                ),
                payloads,
            )
            .map_err(|cause| self.capacity_error(cause))?;

        Ok((completed, abandoned))
    }

    pub(super) fn push_checked_lifecycle_call(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: bray_bound_tree::LifecycleCallable,
    ) -> Result<(bray_ir::MirBlockId, bray_ir::MirValueId), C::Error> {
        let result = callable.result;

        let value = self.push_selected_call(builder, block, source, place, callable, result)?;

        if callable.result.execution() == CallableExecution::Synchronous {
            self.check_lifecycle_value(builder, block, source, value, callable.result.ty())
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
