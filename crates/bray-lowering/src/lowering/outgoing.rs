use super::LoweringError;
use super::lowerer::Lowerer;
use bray_bound_tree::BoundExpressionId;
use bray_ir::{MirBlockId, MirOperationKind, MirSourceAnchor};
use bray_symbols::TypeId;

impl Lowerer<'_> {
    pub(super) fn admit_scoped_exit(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, LoweringError> {
        let boolean =
            self.representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let entered = self.new_initialization_guard(block, source, boolean, &[], false)?;
        let admitted = self.new_initialization_guard(block, source, boolean, &[], false)?;

        self.track_scope_admission(expression, source, Self::retained_place(&admitted));

        self.scoped_guards.insert(
            expression,
            super::scoped::ScopedCleanup {
                entered,
                admitted: Self::retained_place(&admitted),
            },
        );

        let owner = self.scoped_exit_owner(expression);
        let block = self.admit_outgoing(expression, block, source, owner)?;

        self.set_boolean_guard(block, source, &admitted, true)?;

        Ok(block)
    }

    pub(super) fn discharge_scoped_exit(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        expression: BoundExpressionId,
        value: Option<(bray_ir::MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        let guard = Self::retained_place(
            &self
                .scoped_guards
                .get(&expression)
                .expect("scoped cleanup retains its admission guard")
                .admitted,
        );

        let owner = self.scoped_exit_owner(expression);

        self.guarded_cleanup_region(
            block,
            source,
            Self::retained_place(&guard),
            Some(Self::retained_place(&guard)),
            value,
            |lowerer, block, guard, value| {
                lowerer.set_boolean_guard(block, source, &guard, false)?;
                lowerer.discharge_outgoing(block, source, owner)?;

                Ok((block, value))
            },
        )
    }

    fn scoped_exit_owner(&self, expression: BoundExpressionId) -> bray_ir::MirOutgoingOwner {
        let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) =
            self.input.semantic_selections().expression(expression)
        else {
            panic!("scoped admission retains its selected exit");
        };

        let ty = self
            .input
            .semantic_values()
            .type_data(scoped.exit().1.callable_type());

        let bray_symbols::TypeData::Callable(ty) = ty.as_ref() else {
            panic!("selected exit has its callable type");
        };

        bray_ir::MirOutgoingOwner::Invocation {
            callable: bray_ir::MirCallableReference::new(scoped.exit().0, ty.abi()),
            result: scoped.exit_result(),
        }
    }

    pub(in crate::lowering) fn admit_outgoing_owner(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        ty: TypeId,
    ) -> Result<MirBlockId, LoweringError> {
        if self.input.cleanup_type(ty).is_some_and(|shape| {
            shape.cleanup() == bray_bound_tree::AsyncStorageCleanupRequirement::None
        }) {
            return Ok(block);
        }

        self.admit_outgoing(
            expression,
            block,
            source,
            bray_ir::MirOutgoingOwner::Value(ty),
        )
    }

    pub(in crate::lowering) fn admit_outgoing(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
    ) -> Result<MirBlockId, LoweringError> {
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::AdmitOutgoing {
                owner,
                runtime: self
                    .runtime_reference(bray_runtime_interface::RuntimeAbiRole::OutgoingAdmission),
            },
            None,
        )?;

        let unit = self.representation_type(bray_compiler_known::RepresentationRole::Unit)?;

        let (block, _) = self.finish_typed_call_panic_check(
            expression,
            block,
            source,
            &self.unit_operand(unit),
            unit,
            None,
            crate::lowering::expression::CallOutcome::Runtime,
        )?;

        Ok(block)
    }

    pub(in crate::lowering) fn discharge_outgoing_owner(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        ty: TypeId,
    ) -> Result<(), LoweringError> {
        if self.input.cleanup_type(ty).is_some_and(|shape| {
            shape.cleanup() == bray_bound_tree::AsyncStorageCleanupRequirement::None
        }) {
            return Ok(());
        }

        self.discharge_outgoing(block, source, bray_ir::MirOutgoingOwner::Value(ty))
    }

    pub(in crate::lowering) fn discharge_outgoing(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
    ) -> Result<(), LoweringError> {
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::DischargeOutgoing {
                owner,
                runtime: self
                    .runtime_reference(bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge),
            },
            None,
        )?;

        Ok(())
    }
}
