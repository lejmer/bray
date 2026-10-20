use bray_bound_tree::{BoundCallableTarget, BoundExecutionSite, BoundExpressionId, SelectedCall, SelectedImplementationWitness};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirBlockId, MirCall, MirCallArgument, MirCallTarget, MirCallableReference, MirOperand, MirOperationKind, MirPredicateReference, MirSourceAnchor};
use bray_symbols::ReceiverMode;

use super::LoweringError;
use super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn lower_primitive_requirements(
        &mut self, expression: BoundExpressionId, block: MirBlockId, source: &MirSourceAnchor,
        selection: &SelectedCall, values: &mut [MirOperand],
    ) -> Result<MirBlockId, LoweringError> {
        if self.input.storage_flow().preconditions_proven(expression.into())
            || selection.phase_behaviors().invocation().predicate_requirements().iter().all(|requirement| !requirement.requires_runtime_check(self.input.semantic_values(), self.input.available_compiler_known_symbols())) {
            return Ok(block);
        }
        assert_eq!(values.len(), selection.arguments().len() + usize::from(selection.receiver().is_some()), "checked primitive requirement retains every evaluated input: {expression:?}");
        let mut arguments = Vec::with_capacity(values.len());
        let offset = usize::from(selection.receiver().is_some());
        for (index, value) in values.iter_mut().enumerate() {
            let ty = self.builder.operand_type(value);
            // Staging moves the represented input. Only its descriptor is copied here.
            *value = self.materialize_owned_input(block, source, value.clone(), ty)?;
            let argument = if index < offset {
                MirCallArgument::Receiver { parameter: selection.receiver().expect("receiver-first input retains its selected receiver").parameter(), value: value.clone() }
            } else {
                let argument = &selection.arguments()[index - offset];
                let (parameter, ordinal) = match argument {
                    bray_bound_tree::SelectedArgument::Explicit { parameter, ordinal, .. } => (*parameter, *ordinal),
                    bray_bound_tree::SelectedArgument::Default { parameter, ordinal, .. } => (Some(*parameter), *ordinal),
                };
                MirCallArgument::Explicit { parameter, ordinal, value: value.clone() }
            };
            arguments.push(argument);
        }
        self.lower_call_requirements(expression, block, source, selection, &arguments)
    }

    pub(super) fn lower_call_requirements(&mut self, expression: BoundExpressionId, block: MirBlockId,
        source: &MirSourceAnchor, selection: &SelectedCall, arguments: &[MirCallArgument]) -> Result<MirBlockId, LoweringError>
    {
        if self.input.storage_flow().preconditions_proven(expression.into()) { return Ok(block); }
        let references = self.requirement_references(selection, arguments);
        self.lower_requirement_checks(expression.into(), block, source, &references, arguments,
            selection.receiver().map(|receiver| receiver.mode()), selection.witnesses())
    }

    pub(in crate::lowering) fn lower_declared_requirements(&mut self, site: BoundExecutionSite, block: MirBlockId,
        source: &MirSourceAnchor, callable: MirCallableReference, arguments: &[MirCallArgument], receiver: Option<ReceiverMode>) -> Result<MirBlockId, LoweringError>
    {
        if self.input.storage_flow().preconditions_proven(site) { return Ok(block); }
        let references = self.declaration_requirement_references(callable);
        self.lower_requirement_checks(site, block, source, &references, arguments, receiver, &[])
    }

    pub(in crate::lowering) fn lower_protocol_requirements(&mut self, site: BoundExecutionSite, block: MirBlockId,
        source: &MirSourceAnchor, call: MirCall) -> Result<(MirBlockId, MirCall), LoweringError>
    {
        let MirCallTarget::Direct(callable) = call.target() else { return Ok((block, call)); };
        let Some(preconditions) = self.input.declaration_preconditions(callable.instance().definition()) else { return Ok((block, call)); };
        if self.input.storage_flow().preconditions_proven(site)
            && self.input.semantic_selections().invoked_callable(site) == Some(callable.instance()) {
            return Ok((block, call));
        }
        let references = self.declaration_requirement_references(*callable);
        let receiver = preconditions.receiver();
        let call = call.try_map_inputs(|value| {
            let ty = self.builder.operand_type(value);
            // The call keeps its input descriptor after ownership moves into staging.
            self.materialize_owned_input(block, source, value.clone(), ty)
        })?;
        let block = self.lower_requirement_checks(site, block, source, &references, call.arguments(), receiver, call.witnesses())?;
        Ok((block, call))
    }

    fn lower_requirement_checks(&mut self, site: BoundExecutionSite, mut block: MirBlockId,
        source: &MirSourceAnchor, references: &[MirPredicateReference], arguments: &[MirCallArgument],
        receiver: Option<ReceiverMode>, witnesses: &[SelectedImplementationWitness]) -> Result<MirBlockId, LoweringError>
    {
        if references.is_empty() { return Ok(block); }
        let expression = site.expression().expect("source requirement retains its invocation expression");
        let inputs = self.borrow_staged_inputs(expression, block, source, receiver,
            arguments.iter().enumerate().map(|(index, argument)| match argument {
                MirCallArgument::Receiver { value, .. } => (None, value),
                MirCallArgument::Explicit { value, .. } if receiver.is_some() && index == 0 => (None, value),
                MirCallArgument::Explicit { ordinal, value, .. } => (Some(*ordinal), value),
            }))?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        for reference in references {
            let condition = self.requirement_value(expression, block, source, reference, &inputs, witnesses)?;
            let (continued, condition) = self.finish_typed_call_panic_check(site, block,
                source, &condition, boolean, None, crate::lowering::expression::CallOutcome::Predicate)?;
            block = continued;
            let (success, failure) = self.branch_on_boolean(block, source, condition)?;
            self.finish_assertion_failure(expression, None, failure, source)?;
            block = success;
        }
        Ok(block)
    }

    fn requirement_value(&mut self, expression: BoundExpressionId, block: MirBlockId,
        source: &MirSourceAnchor, reference: &MirPredicateReference, inputs: &[MirOperand],
        witnesses: &[SelectedImplementationWitness]) -> Result<MirOperand, LoweringError> {
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        // Each clause shares readonly descriptors of the same evaluated inputs.
        let predicate = MirCall::protocol(MirCallTarget::Predicate(reference.clone()),
            bray_bound_tree::BoundCallResult::Immediate(boolean), inputs.iter().cloned(), witnesses.iter().copied());
        self.push_typed_value_operation(expression, block, Self::retained_source(source), MirOperationKind::Call(predicate), boolean)
    }

    pub(super) fn lower_scoped_exit_requirements(&mut self, expression: BoundExpressionId,
        mut block: MirBlockId, source: &MirSourceAnchor, callable: MirCallableReference,
        place: &bray_ir::MirPlace, shielded: bool, continuations: &mut Vec<MirBlockId>) -> Result<MirBlockId, LoweringError> {
        if self.input.storage_flow().preconditions_proven(BoundExecutionSite::ScopedExit(expression)) { return Ok(block); }
        let references = self.declaration_requirement_references(callable);
        if references.is_empty() { return Ok(block); }
        let value = MirOperand::Move(Self::retained_place(place));
        let inputs = self.borrow_staged_inputs(expression, block, source, None, [(Some(0), &value)])?;
        for reference in references {
            let condition = self.requirement_value(expression, block, source, &reference, &inputs, &[])?;
            let (continued, condition) = self.check_scoped_exit_call(expression, block, source, shielded, continuations, false, condition)?;
            let (success, failure) = self.branch_on_boolean(continued, source, condition)?;
            let report_type = self.representation_type(RepresentationRole::PanicReport)?;
            let report = self.push_panic_report(expression, failure, source, bray_ir::MirPanicCause::Assertion(None), report_type)?;
            if let Some(outcome) = &self.cleanup_outcome {
                let continuation = self.builder.push_block(Self::retained_source(source), self.builder.block_kind(failure))?;
                outcome.retain_report(&mut self.builder, failure, source, bray_ir::MirEdge::new(continuation, []), bray_ir::MirCleanupState::Panicked, report)?;
                continuations.push(continuation);
            } else {
                let (panicked, _, destination) = self.cleanup_failure_targets.clone()
                    .expect("ordinary scoped exit retains its prepared cleanup failure targets");
                self.push_operation(failure, Self::retained_source(source), MirOperationKind::Store {
                    kind: bray_ir::MirStoreKind::Initialize, destination, value: report,
                }, None)?;
                if shielded { self.cleanup_shield(failure, source, bray_runtime_interface::RuntimeAbiRole::CleanupShieldLeave)?; }
                self.set_terminator(failure, Self::retained_source(source), bray_ir::MirTerminatorKind::Goto(bray_ir::MirEdge::new(panicked, [])))?;
            }
            block = success;
        }
        Ok(block)
    }

    fn declaration_requirement_references(&self, callable: MirCallableReference) -> Vec<MirPredicateReference> {
        self.input.declaration_preconditions(callable.instance().definition()).into_iter()
            .flat_map(|preconditions| preconditions.clauses()).map(|ordinal| MirPredicateReference::Clause { callable, ordinal: *ordinal }).collect()
    }

    fn requirement_references(&self, selection: &SelectedCall, arguments: &[MirCallArgument]) -> Vec<MirPredicateReference> {
        if let BoundCallableTarget::Declaration(instance) = selection.target() {
            return self.declaration_requirement_references(MirCallableReference::new(instance, selection.abi()));
        }
        let mut inputs = arguments.iter().map(|argument| {
            let (ordinal, value) = match argument {
                MirCallArgument::Receiver { value, .. } => (None, value),
                MirCallArgument::Explicit { ordinal, value, .. } => (Some(*ordinal), value),
            };
            let MirOperand::Move(place) = value else { panic!("requirement observes staged caller-owned inputs"); };
            (ordinal, place.ty())
        }).collect::<Vec<_>>();
        inputs.sort_by_key(|(ordinal, _)| *ordinal);
        selection.phase_behaviors().invocation().predicate_requirements().iter()
            .filter(|requirement| requirement.requires_runtime_check(self.input.semantic_values(), self.input.available_compiler_known_symbols()))
            .map(|requirement| MirPredicateReference::condition(requirement.condition().expect("erased callable requirement retains its checked executable condition"), inputs.iter().map(|(_, ty)| *ty))).collect()
    }
}
