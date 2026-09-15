use super::core::UnitTranslator;
use super::support::{
    extract_value, int_value, integer_constant, llvm, pointer_value, result_type,
};
use bray_codegen::{
    CodegenCallSite, CodegenFailure, CodegenResultMapping, CodegenSymbolKey, CodegenTypeKind,
};
use bray_ir::{MirBlockId, MirEdge, MirOperand, MirPatternPredicate, MirPlace, MirTerminatorKind};
use inkwell::values::{BasicValueEnum, IntValue, PointerValue, StructValue};
use inkwell::{FloatPredicate, IntPredicate};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_terminator(
        &mut self,
        block: MirBlockId,
        terminator: &MirTerminatorKind,
    ) -> Result<(), CodegenFailure> {
        // Keep this exhaustive so every MIR terminator requires an explicit translation.
        match terminator {
            MirTerminatorKind::Goto(edge) => self.translate_goto(edge)?,
            MirTerminatorKind::BeginCleanup(cleanup)
            | MirTerminatorKind::ContinueCleanup(cleanup) => self.translate_goto(cleanup.edge())?,
            MirTerminatorKind::Branch {
                condition,
                then_edge,
                else_edge,
            } => {
                let condition = int_value(self.operand(condition)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let (source, pending_moves) = self.take_control_source()?;

                let then_route = self.route_edge(then_edge, "branch.then", &pending_moves)?;
                let else_route = self.route_edge(else_edge, "branch.else", &pending_moves)?;

                self.builder.position_at_end(source);

                llvm(
                    self.builder
                        .build_conditional_branch(condition, then_route, else_route),
                )?;
            }
            MirTerminatorKind::Switch {
                discriminant,
                cases,
                otherwise,
            } => {
                let discriminant = int_value(self.operand(discriminant)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let case_values = cases
                    .iter()
                    .map(|case| {
                        int_value(self.constant(case.value())?)
                            .filter(|value| value.get_type() == discriminant.get_type())
                            .ok_or(CodegenFailure::GeneratedModuleInvariant)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let (source, pending_moves) = self.take_control_source()?;

                let otherwise_route =
                    self.route_edge(otherwise, "switch.otherwise", &pending_moves)?;

                let mut llvm_cases = Vec::with_capacity(cases.len());

                for (index, (case, value)) in cases.iter().zip(case_values).enumerate() {
                    let route = self.route_edge(
                        case.edge(),
                        &format!("switch.case.{index}"),
                        &pending_moves,
                    )?;

                    llvm_cases.push((value, route));
                }

                self.builder.position_at_end(source);

                llvm(
                    self.builder
                        .build_switch(discriminant, otherwise_route, &llvm_cases),
                )?;
            }
            MirTerminatorKind::Return(value) => {
                if self.frame_context.is_some() {
                    if value.is_some() {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    }

                    let progress = self
                        .frame_progress
                        .take()
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    self.return_frame_progress(progress)?;
                } else {
                    self.translate_return(value.as_ref())?;
                }
            }
            MirTerminatorKind::Unreachable => {
                self.clear_moved_places()?;
                llvm(self.builder.build_unreachable())?;
            }
            MirTerminatorKind::InlineAssembly(assembly) => {
                self.translate_inline_assembly_terminator(block, assembly)?;
            }
            MirTerminatorKind::Panic { cleanup, .. }
            | MirTerminatorKind::CancelCurrentRun { cleanup } => {
                self.add_edge_arguments(cleanup.edge())?;
                self.clear_moved_places()?;

                llvm(
                    self.builder
                        .build_unconditional_branch(self.block(cleanup.edge().target())?),
                )?;
            }
            MirTerminatorKind::PropagatePanic { report, runtime } => {
                self.translate_panic_propagation(report, *runtime)?;
            }
            MirTerminatorKind::PropagateCancellation { runtime } => {
                self.translate_cancellation_propagation(*runtime)?;
            }
            MirTerminatorKind::PatternBranch {
                subject,
                predicate,
                matched,
                unmatched,
            } => {
                let condition = match (predicate, subject) {
                    (
                        MirPatternPredicate::NullablePresent | MirPatternPredicate::NullableAbsent,
                        MirOperand::Copy(place),
                    ) => {
                        let address = self.place(place)?;
                        let present = self.nullable_present_at(address, place.ty())?;

                        if *predicate == MirPatternPredicate::NullablePresent {
                            present
                        } else {
                            llvm(self.builder.build_not(present, "pattern.nullable.absent"))?
                        }
                    }
                    _ => {
                        let subject_type = self.operand_type(subject)?;
                        let subject = self.observed_operand(subject)?;

                        self.translate_pattern_predicate(block, subject, subject_type, *predicate)?
                    }
                };

                let (source, pending_moves) = self.take_control_source()?;

                let matched_route = self.route_edge(matched, "pattern.matched", &pending_moves)?;

                let unmatched_route =
                    self.route_edge(unmatched, "pattern.unmatched", &pending_moves)?;

                self.builder.position_at_end(source);

                llvm(self.builder.build_conditional_branch(
                    condition,
                    matched_route,
                    unmatched_route,
                ))?;
            }
            MirTerminatorKind::Iterate {
                cursor,
                next: _,
                element_type,
                item,
                exhausted,
                ..
            } => {
                self.translate_iteration(block, cursor, *element_type, *item, exhausted)?;
            }
            MirTerminatorKind::RangeIterate {
                cursor,
                element_type,
                item,
                exhausted,
            } => {
                self.translate_range_iteration(cursor, *element_type, *item, exhausted)?;
            }
            MirTerminatorKind::Suspend {
                kind,
                payload,
                resume_state,
                registration,
                wake,
                ..
            } => {
                self.translate_suspension(
                    *kind,
                    payload.as_ref(),
                    *resume_state,
                    *registration,
                    *wake,
                )?;
            }
            MirTerminatorKind::ForwardRunResult { result, edges } => {
                let result_type = self.operand_type(result)?;
                let result = self.operand(result)?;
                let tag = self.union_tag(result, result_type)?;

                let completed =
                    self.union_variant_tag(result_type, edges.completed_variant(), tag.get_type())?;

                let panicked =
                    self.union_variant_tag(result_type, edges.panicked_variant(), tag.get_type())?;

                let cancelled =
                    self.union_variant_tag(result_type, edges.cancelled_variant(), tag.get_type())?;

                let (source, pending_moves) = self.take_control_source()?;

                let completed_route =
                    self.route_edge(edges.completed(), "run.completed", &pending_moves)?;

                let panicked_route =
                    self.route_edge(edges.panicked().edge(), "run.panicked", &pending_moves)?;

                let cancelled_route =
                    self.route_edge(edges.cancelled().edge(), "run.cancelled", &pending_moves)?;

                let cases = [
                    (completed, completed_route),
                    (panicked, panicked_route),
                    (cancelled, cancelled_route),
                ];

                self.builder.position_at_end(source);

                llvm(self.builder.build_switch(tag, cancelled_route, &cases))?;
            }
            MirTerminatorKind::CheckCallOutcome {
                completed,
                panicked,
                cancelled,
            } => self.translate_call_panic(block, completed, *panicked, cancelled)?,
        }

        Ok(())
    }

    fn translate_suspension(
        &mut self,
        kind: bray_ir::MirSuspensionKind,
        payload: Option<&MirOperand>,
        resume_state: bray_ir::MirFrameStateId,
        registration: bray_ir::MirRuntimeReference,
        wake: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        if let Some(frame_context) = self.frame_context {
            let context = self.frame_context_argument()?;

            let pointer = llvm(
                self.builder.build_int_to_ptr(
                    context,
                    self.types
                        .context()
                        .ptr_type(inkwell::AddressSpace::default()),
                    "frame.context",
                ),
            )?;

            let state_pointer = llvm(self.builder.build_struct_gep(
                frame_context,
                pointer,
                0,
                "frame.state.pointer",
            ))?;

            let state = self
                .types
                .context()
                .i32_type()
                .const_int(u64::from(resume_state.raw()), false);

            llvm(self.builder.build_store(state_pointer, state))?;

            let progress_kind = match kind {
                bray_ir::MirSuspensionKind::Awaited => {
                    bray_runtime_abi::NativeFrameProgressKind::SUSPENDED
                }
                bray_ir::MirSuspensionKind::Yield => {
                    bray_runtime_abi::NativeFrameProgressKind::YIELDED
                }
                bray_ir::MirSuspensionKind::TaskEvent => {
                    bray_runtime_abi::NativeFrameProgressKind::TASK_EVENT
                }
            };

            let payload = match payload.map(|payload| self.operand(payload)).transpose()? {
                Some(BasicValueEnum::IntValue(payload)) => payload,
                Some(_) => return Err(CodegenFailure::GeneratedModuleInvariant),
                None => self.types.context().i64_type().const_zero(),
            };

            let payload = if payload.get_type().get_bit_width() == 64 {
                payload
            } else {
                llvm(self.builder.build_int_z_extend(
                    payload,
                    self.types.context().i64_type(),
                    "suspension.payload",
                ))?
            };

            let progress =
                self.build_frame_progress(progress_kind.code(), state, payload.into())?;

            self.return_frame_progress(progress.into())?;

            return Ok(());
        }

        self.runtime_function_pointer(wake)?;

        let state =
            self.runtime_integer_argument(registration, 0, u64::from(resume_state.raw()))?;

        let outcome = self
            .invoke_runtime(registration, &[state])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.return_machine_value(outcome)
    }

    pub(super) fn build_frame_progress(
        &self,
        kind: u32,
        state: IntValue<'context>,
        payload: BasicValueEnum<'context>,
    ) -> Result<StructValue<'context>, CodegenFailure> {
        let mut progress = crate::native::frame_progress_type(self.types.context()).const_zero();

        let fields: [BasicValueEnum<'context>; 2] = [
            self.types
                .context()
                .i32_type()
                .const_int(u64::from(kind), false)
                .into(),
            state.into(),
        ];

        for (index, field) in fields.into_iter().enumerate() {
            progress = llvm(self.builder.build_insert_value(
                progress,
                field,
                crate::conversion::resource_limit(index, "frame_progress_field_index")?,
                "frame.progress.field",
            ))?
            .into_struct_value();
        }

        let payload_index = if kind == bray_runtime_abi::NativeFrameProgressKind::PANICKED.code() {
            3
        } else {
            2
        };

        Ok(llvm(self.builder.build_insert_value(
            progress,
            payload,
            payload_index,
            "frame.progress.payload",
        ))?
        .into_struct_value())
    }

    pub(super) fn translate_iteration(
        &mut self,
        block: MirBlockId,
        cursor: &MirPlace,
        element_type: bray_symbols::TypeId,
        item: MirBlockId,
        exhausted: &MirEdge,
    ) -> Result<(), CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .callable(self.instance.key(), CodegenCallSite::Terminator(block))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let symbol = self
            .request
            .mappings()
            .instance_symbol(
                mapping
                    .instance()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
            )
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let cursor = self.place(cursor)?.into();

        let result = self
            .invoke_function(function, symbol.signature(), &[cursor], "iterate.next")?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let result_type = result_type(symbol.signature())?;
        let present = self.nullable_present(result, result_type)?;
        let element = self.project_value(result, result_type, element_type)?;

        let item_block = self
            .unit
            .block(item)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let [parameter] = item_block.parameters() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let phi = self
            .phis
            .get(parameter)
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let (source, pending_moves) = self.take_control_source()?;

        let item_route = self.route_target(item, "iterate.item", &pending_moves)?;
        let exhausted_route = self.route_edge(exhausted, "iterate.exhausted", &pending_moves)?;

        phi.add_incoming(&[(&element, item_route)]);
        self.builder.position_at_end(source);

        llvm(
            self.builder
                .build_conditional_branch(present, item_route, exhausted_route),
        )?;

        Ok(())
    }

    pub(super) fn runtime_function_pointer(
        &self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok(function.as_global_value().as_pointer_value())
    }

    pub(super) fn translate_pattern_predicate(
        &mut self,
        block: MirBlockId,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        predicate: MirPatternPredicate,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        match predicate {
            MirPatternPredicate::Literal(_) => {
                let mapping = self
                    .request
                    .mappings()
                    .terminator(self.instance.key(), block)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let [literal] = mapping.constants() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let literal = self.constant(*literal)?;

                self.equal_values(subject, literal)
            }
            MirPatternPredicate::Constant(term) => {
                let value = self
                    .request
                    .mappings()
                    .constant_term(self.instance.key(), term)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let value = self.constant(value)?;

                self.equal_values(subject, value)
            }
            MirPatternPredicate::NullableAbsent | MirPatternPredicate::NullablePresent => {
                let present = self.nullable_present(subject, subject_type)?;

                if predicate == MirPatternPredicate::NullablePresent {
                    Ok(present)
                } else {
                    llvm(self.builder.build_not(present, "pattern.nullable.absent"))
                }
            }
            MirPatternPredicate::ActiveUnionVariant(variant) => {
                self.active_union_variant(subject, subject_type, variant)
            }
            MirPatternPredicate::ProductShape(_)
            | MirPatternPredicate::TupleShape(_)
            | MirPatternPredicate::ArrayShape(_)
            | MirPatternPredicate::OwnedTarget => {
                Ok(self.types.context().bool_type().const_int(1, false))
            }
        }
    }

    pub(super) fn equal_values(
        &self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        if left.get_type() != right.get_type() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        match (left, right) {
            (BasicValueEnum::IntValue(left), BasicValueEnum::IntValue(right)) => llvm(
                self.builder
                    .build_int_compare(IntPredicate::EQ, left, right, "pattern.equal"),
            ),
            (BasicValueEnum::FloatValue(left), BasicValueEnum::FloatValue(right)) => llvm(
                self.builder
                    .build_float_compare(FloatPredicate::OEQ, left, right, "pattern.equal"),
            ),
            (BasicValueEnum::PointerValue(left), BasicValueEnum::PointerValue(right)) => llvm(
                self.builder
                    .build_int_compare(IntPredicate::EQ, left, right, "pattern.equal"),
            ),
            (BasicValueEnum::ArrayValue(left), BasicValueEnum::ArrayValue(right)) => {
                self.equal_aggregate(left.into(), right.into(), left.get_type().len())
            }
            (BasicValueEnum::StructValue(left), BasicValueEnum::StructValue(right)) => {
                self.equal_aggregate(left.into(), right.into(), left.get_type().count_fields())
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn equal_aggregate(
        &self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
        length: u32,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let mut equal = self.types.context().bool_type().const_int(1, false);

        for index in 0..length {
            let left = extract_value(&self.builder, left, index)?;
            let right = extract_value(&self.builder, right, index)?;
            let field_equal = self.equal_values(left, right)?;

            equal = llvm(
                self.builder
                    .build_and(equal, field_equal, "pattern.equal.all"),
            )?;
        }

        Ok(equal)
    }

    pub(super) fn active_union_variant(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let tag = self.union_tag(subject, subject_type)?;
        let expected = self.union_variant_tag(subject_type, variant, tag.get_type())?;

        llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            tag,
            expected,
            "pattern.union.active",
        ))
    }

    pub(super) fn union_tag(
        &mut self,
        mut subject: BasicValueEnum<'context>,
        mut subject_type: bray_symbols::TypeId,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let (storage, tag_type) = loop {
            let mapping = self
                .type_mapping(subject_type)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            match mapping.kind() {
                CodegenTypeKind::Union { tag, .. } => {
                    let Some(tag) = *tag else {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    };

                    let storage =
                        self.allocate_temporary(subject.get_type(), "pattern.union.subject")?;

                    llvm(self.builder.build_store(storage, subject))?;

                    break (storage, tag);
                }
                CodegenTypeKind::Pointer { target, .. } => {
                    let storage =
                        pointer_value(subject).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let target_mapping = self
                        .type_mapping(*target)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    if let CodegenTypeKind::Union { tag, .. } = target_mapping.kind() {
                        let Some(tag) = *tag else {
                            return Err(CodegenFailure::GeneratedModuleInvariant);
                        };

                        break (storage, tag);
                    }

                    subject = llvm(self.builder.build_load(
                        self.types.map(*target)?,
                        storage,
                        "pattern.union.borrow",
                    ))?;

                    subject_type = *target;
                }
                _ => return Err(CodegenFailure::GeneratedModuleInvariant),
            }
        };

        let mapped_tag = self.types.map(tag_type)?;

        let tag = llvm(
            self.builder
                .build_load(mapped_tag, storage, "pattern.union.tag"),
        )?;

        let tag = int_value(tag).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok(tag)
    }

    pub(super) fn union_variant_tag(
        &self,
        mut subject_type: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        tag_type: inkwell::types::IntType<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let kind = loop {
            let mapping = self
                .type_mapping(subject_type)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            match mapping.kind() {
                kind @ CodegenTypeKind::Union { .. } => break kind,
                CodegenTypeKind::Pointer { target, .. } => subject_type = *target,
                _ => return Err(CodegenFailure::GeneratedModuleInvariant),
            }
        };

        let tag = kind
            .union_variant(variant)
            .and_then(bray_codegen::CodegenUnionVariantLayout::tag)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok(integer_constant(tag_type, tag))
    }

    pub(super) fn translate_return(
        &mut self,
        value: Option<&MirOperand>,
    ) -> Result<(), CodegenFailure> {
        match self.signature.result() {
            CodegenResultMapping::Void => {
                if let Some(value) = value {
                    let ty = self.operand_type(value)?;

                    if self.mapped_type_size(ty)? != 0 {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    }
                }

                self.clear_moved_places()?;
                llvm(self.builder.build_return(None))?;
            }
            CodegenResultMapping::Direct { .. } => {
                let value = value.ok_or(CodegenFailure::GeneratedModuleInvariant)?;
                let value = self.operand(value)?;

                self.clear_moved_places()?;

                llvm(self.builder.build_return(Some(&value)))?;
            }
            CodegenResultMapping::Indirect { pointee, .. } => {
                let value = value.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let destination = self
                    .function
                    .get_first_param()
                    .and_then(pointer_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let source_type = self.operand_type(value)?;
                let value = self.operand(value)?;
                let value = self.convert(value, source_type, *pointee)?;

                llvm(self.builder.build_store(destination, value))?;
                self.clear_moved_places()?;
                llvm(self.builder.build_return(None))?;
            }
        }

        Ok(())
    }

    fn return_machine_value(
        &mut self,
        value: BasicValueEnum<'context>,
    ) -> Result<(), CodegenFailure> {
        match self.signature.result() {
            CodegenResultMapping::Direct { .. } => {
                llvm(self.builder.build_return(Some(&value)))?;
            }
            CodegenResultMapping::Indirect { .. } => {
                let destination = self
                    .function
                    .get_first_param()
                    .and_then(pointer_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_store(destination, value))?;
                llvm(self.builder.build_return(None))?;
            }
            CodegenResultMapping::Void => {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }

        Ok(())
    }
}
