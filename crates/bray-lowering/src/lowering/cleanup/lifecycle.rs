use bray_bound_tree::LifecycleCallable;
use bray_ir::{
    MirBlockId, MirCallableReference, MirEdge, MirOperand, MirOperationKind, MirPlace,
    MirSourceAnchor, MirTerminatorKind,
};

use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn push_async_finalizer(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: LifecycleCallable,
    ) -> Result<MirBlockId, LoweringError> {
        let occurrence = self
            .cleanup_occurrence
            .expect("async finalization retains its ownership-end occurrence");

        let local = self.cleanup_outcome.is_none();

        let outcome = match &self.cleanup_outcome {
            // Nested resolution shares the enclosing outcome's immutable storage paths.
            Some(outcome) => outcome.clone(),
            None => self.create_cleanup_outcome(block, source)?,
        };

        let invocation = callable.invocation_result(
            self.input.semantic_values(),
            self.input.available_compiler_known_symbols(),
        )?;

        let receiver = self.input.semantic_values().type_data(callable.receiver);

        let borrow = match receiver.as_ref() {
            bray_symbols::TypeData::Borrow { kind, .. } => Some(*kind),
            _ => None,
        };

        let value = crate::lifecycle_call::lower_lifecycle_call(
            MirCallableReference::new(callable.callable, callable.abi),
            callable.receiver,
            Self::retained_place(&place),
            borrow,
            invocation,
            true,
            |operation, ty| {
                self.push_operation(block, Self::retained_source(source), operation, ty)
            },
        )?;

        let awaited = self.lower_awaited_frame(
            occurrence,
            MirOperand::Value(value),
            callable.result,
            block,
            Self::retained_source(source),
        )?;

        let block = awaited
            .block
            .expect("shielded finalization resumes cleanup");

        let value = awaited
            .value
            .expect("shielded finalization publishes its completion result");

        let payloads = crate::operand::result_payloads(
            self.input.semantic_values(),
            self.input.available_compiler_known_symbols(),
            callable.result,
        );

        let block =
            outcome.retain_finalizer_result(&mut self.builder, block, source, value, payloads)?;

        // Keep the owner broadcastable throughout the awaited mutation. Destruction
        // retires its guard only after every finalizer outcome has reached this point.
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Destroy(place),
            None,
        )?;

        let block = outcome.check(&mut self.builder, block, source)?;

        if local {
            self.finish_finalizer_outcome(block, source, &outcome)
        } else {
            Ok(block)
        }
    }

    fn finish_finalizer_outcome(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &CleanupOutcome,
    ) -> Result<MirBlockId, LoweringError> {
        // Failure handlers own their report storage independently of this local outcome.
        let (panicked, cancelled, report) = self
            .cleanup_failure_targets
            .clone()
            .expect("ordinary finalization retains the surrounding cleanup failure targets");

        let panic = self.builder.push_block(
            Self::retained_source(source),
            self.builder.block_kind(block),
        )?;

        self.push_operation(
            panic,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination: report,
                value: outcome.report(),
            },
            None,
        )?;

        self.set_terminator(
            panic,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(panicked, [])),
        )?;

        Ok(outcome.dispatch(&mut self.builder, block, source, panic, cancelled)?)
    }
}
