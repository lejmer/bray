use super::core::UnitTranslator;
use super::support::{extract_value, int_value, integer_constant, llvm, pointer_value};
use bray_codegen::{CodegenFailure, CodegenResultMapping, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::{MirBlockId, MirOperand, MirPatternPredicate, MirTerminatorKind};
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
                let condition = int_value(self.operand(condition)?).expect(
                    "checked MIR control translation requires an established mapping or value",
                );

                let (source, moved_reports) = self.take_control_source()?;

                let then_route = self.route_edge(then_edge, "branch.then", &moved_reports)?;
                let else_route = self.route_edge(else_edge, "branch.else", &moved_reports)?;

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
                let discriminant = int_value(self.operand(discriminant)?).expect(
                    "checked MIR control translation requires an established mapping or value",
                );

                let case_values = cases
                    .iter()
                    .map(|case| {
                        Ok(int_value(self.constant(case.value())?)
                            .filter(|value| value.get_type() == discriminant.get_type())
                            .expect(
                                "switch cases must use the discriminant's represented integer type",
                            ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let (source, moved_reports) = self.take_control_source()?;

                let otherwise_route =
                    self.route_edge(otherwise, "switch.otherwise", &moved_reports)?;

                let mut llvm_cases = Vec::with_capacity(cases.len());

                for (index, (case, value)) in cases.iter().zip(case_values).enumerate() {
                    let route = self.route_edge(
                        case.edge(),
                        &format!("switch.case.{index}"),
                        &moved_reports,
                    )?;

                    llvm_cases.push((value, route));
                }

                self.builder.position_at_end(source);

                llvm(
                    self.builder
                        .build_switch(discriminant, otherwise_route, &llvm_cases),
                )?;
            }
            MirTerminatorKind::Return { value, report } => {
                self.translate_return(value.as_ref(), report.as_ref())?
            }
            MirTerminatorKind::Unreachable => {
                self.clear_moved_reports()?;
                llvm(self.builder.build_unreachable())?;
            }
            MirTerminatorKind::InlineAssembly(assembly) => {
                self.translate_inline_assembly_terminator(block, assembly)?;
            }
            MirTerminatorKind::Panic { cleanup, .. }
            | MirTerminatorKind::CancelCurrentRun { cleanup } => {
                self.add_edge_arguments(cleanup.edge())?;
                self.clear_moved_reports()?;

                llvm(
                    self.builder
                        .build_unconditional_branch(self.block(cleanup.edge().target())),
                )?;
            }
            MirTerminatorKind::PropagatePanic { report, runtime } => {
                self.translate_panic_propagation(report, *runtime)?;
            }
            MirTerminatorKind::PropagateCancellation { report, .. } => {
                self.translate_cancellation_propagation(report)?;
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
                        let subject_type = self.operand_type(subject);
                        let subject = self.observed_operand(subject)?;

                        self.translate_pattern_predicate(block, subject, subject_type, *predicate)?
                    }
                };

                let (source, moved_reports) = self.take_control_source()?;

                let matched_route = self.route_edge(matched, "pattern.matched", &moved_reports)?;

                let unmatched_route =
                    self.route_edge(unmatched, "pattern.unmatched", &moved_reports)?;

                self.builder.position_at_end(source);

                llvm(self.builder.build_conditional_branch(
                    condition,
                    matched_route,
                    unmatched_route,
                ))?;
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
                let result_type = self.operand_type(result);
                let result = self.operand(result)?;
                let tag = self.union_tag(result, result_type)?;

                let completed =
                    self.union_variant_tag(result_type, edges.completed_variant(), tag.get_type())?;

                let panicked =
                    self.union_variant_tag(result_type, edges.panicked_variant(), tag.get_type())?;

                let cancelled =
                    self.union_variant_tag(result_type, edges.cancelled_variant(), tag.get_type())?;

                let (source, moved_reports) = self.take_control_source()?;

                let completed_route =
                    self.route_edge(edges.completed(), "run.completed", &moved_reports)?;

                let panicked_route =
                    self.route_edge(edges.panicked().edge(), "run.panicked", &moved_reports)?;

                let cancelled_route =
                    self.route_edge(edges.cancelled().edge(), "run.cancelled", &moved_reports)?;

                let cases = [
                    (completed, completed_route),
                    (panicked, panicked_route),
                    (cancelled, cancelled_route),
                ];

                self.builder.position_at_end(source);

                llvm(self.builder.build_switch(tag, cancelled_route, &cases))?;
            }
            MirTerminatorKind::TakeCallReport { completed, report } => {
                self.translate_normal_call_report(completed, report)?;
            }
            MirTerminatorKind::CheckCallOutcome {
                completed,
                panicked,
                cancelled,
                completed_report,
            } => self.translate_call_panic(
                block,
                completed,
                completed_report.as_ref(),
                panicked,
                cancelled,
            )?,
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
            let pointer = self.frame_context_pointer()?;

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
                Some(_) => panic!(
                    "checked MIR control translation violated an established compiler contract"
                ),
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
            .expect("checked MIR control translation requires an established mapping or value");

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

        let payload_index = if kind == bray_runtime_abi::NativeFrameProgressKind::PANICKED.code()
            || kind == bray_runtime_abi::NativeFrameProgressKind::CANCELLED.code()
        {
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

    pub(super) fn runtime_function_pointer(
        &self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .expect("checked MIR control translation requires an established mapping or value");

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .expect("checked MIR control translation requires an established mapping or value");

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
                    .expect(
                        "checked MIR control translation requires an established mapping or value",
                    );

                let [literal] = mapping.constants() else {
                    panic!(
                        "checked MIR control translation violated an established compiler contract"
                    );
                };

                let literal = self.constant(*literal)?;

                self.equal_pattern_values(subject, literal, subject_type)
            }
            MirPatternPredicate::Constant(term) => {
                let value = self
                    .request
                    .mappings()
                    .constant_term(self.instance.key(), term)
                    .expect(
                        "checked MIR control translation requires an established mapping or value",
                    );

                let value = self.constant(value)?;

                self.equal_pattern_values(subject, value, subject_type)
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
            panic!("checked MIR control translation violated an established compiler contract");
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
            unexpected => panic!(
                "checked MIR control translation violated an established compiler contract: {unexpected:?}"
            ),
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
                .expect("checked MIR control translation requires an established mapping or value");

            match mapping.kind() {
                CodegenTypeKind::Union { tag, .. } => {
                    let Some(tag) = *tag else {
                        panic!(
                            "checked MIR control translation violated an established compiler contract"
                        );
                    };

                    let storage = if self.types.is_address_value(subject_type)? {
                        pointer_value(subject).expect("union value retains its storage")
                    } else {
                        let storage =
                            self.allocate_temporary(subject.get_type(), "pattern.union.subject")?;

                        llvm(self.builder.build_store(storage, subject))?;

                        storage
                    };

                    break (storage, tag);
                }
                CodegenTypeKind::Pointer { target, .. } => {
                    let storage = pointer_value(subject).expect(
                        "checked MIR control translation requires an established mapping or value",
                    );

                    let target_mapping = self.type_mapping(*target).expect(
                        "checked MIR control translation requires an established mapping or value",
                    );

                    if let CodegenTypeKind::Union { tag, .. } = target_mapping.kind() {
                        let Some(tag) = *tag else {
                            panic!(
                                "checked MIR control translation violated an established compiler contract"
                            );
                        };

                        break (storage, tag);
                    }

                    subject = self.place_value(storage, *target, "pattern.union.borrow")?;

                    subject_type = *target;
                }
                unexpected => panic!(
                    "checked MIR control translation violated an established compiler contract: {unexpected:?}"
                ),
            }
        };

        let mapped_tag = self.types.map(tag_type)?;

        let tag = llvm(
            self.builder
                .build_load(mapped_tag, storage, "pattern.union.tag"),
        )?;

        let tag = int_value(tag)
            .expect("checked MIR control translation requires an established mapping or value");

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
                .expect("checked MIR control translation requires an established mapping or value");

            match mapping.kind() {
                kind @ CodegenTypeKind::Union { .. } => break kind,
                CodegenTypeKind::Pointer { target, .. } => subject_type = *target,
                unexpected => panic!(
                    "checked MIR control translation violated an established compiler contract: {unexpected:?}"
                ),
            }
        };

        let tag = kind
            .union_variant(variant)
            .and_then(bray_codegen::CodegenUnionVariantLayout::tag)
            .expect("checked MIR control translation requires an established mapping or value");

        Ok(integer_constant(tag_type, tag))
    }

    pub(super) fn translate_return(
        &mut self,
        value: Option<&MirOperand>,
        report: Option<&MirOperand>,
    ) -> Result<(), CodegenFailure> {
        if self.frame_context.is_some() {
            assert!(
                report.is_none(),
                "protected frames publish owned reports with their terminal state"
            );

            if value.is_some() {
                panic!("checked MIR control translation violated an established compiler contract");
            }

            if self.frame_broadcast {
                self.return_broadcast_outcome(
                    bray_runtime_abi::NativeFrameProgressKind::COMPLETED,
                    self.types.context().i64_type().const_zero().into(),
                )?;
            } else {
                let progress = self
                    .frame_progress
                    .take()
                    .expect("source frame return retains its terminal progress");

                self.return_frame_progress(progress)?;
            }

            return Ok(());
        }

        if let Some(report) = report {
            let report = self.operand(report)?;

            if let Some(destination) = self.panic_report_context {
                let outcome = super::support::native_run_outcome_value(
                    self.types.context(),
                    &self.builder,
                    self.request.target(),
                    bray_runtime_abi::NativeRunState::COMPLETED,
                    None,
                    Some(report),
                )?;

                llvm(self.builder.build_store(destination, outcome))?;
            } else {
                assert_ne!(
                    self.signature.abi(),
                    bray_symbols::CallableAbi::Bray,
                    "Bray report return in {:?} retains its explicit outcome destination",
                    self.instance.key()
                );

                super::callback::consume_owned_report(
                    self.types.context(),
                    self.module,
                    self.request,
                    &self.builder,
                    self.function,
                    report,
                )?;
            }
        }

        match self.signature.result() {
            CodegenResultMapping::Void => {
                if let Some(value) = value {
                    let ty = self.operand_type(value);

                    if self.mapped_type_size(ty) != 0 {
                        panic!(
                            "checked MIR control translation violated an established compiler contract"
                        );
                    }
                }

                self.clear_moved_reports()?;
                llvm(self.builder.build_return(None))?;
            }
            CodegenResultMapping::Direct { ty, coercion, .. } => {
                let value = value.expect(
                    "checked MIR control translation requires an established mapping or value",
                );

                let value = self.operand(value)?;

                let value = match coercion {
                    Some(coercion) => self.encode_abi_result(value, *ty, coercion)?,
                    None => self.abi_register_value(value, *ty)?,
                };

                self.clear_moved_reports()?;

                llvm(self.builder.build_return(Some(&value)))?;
            }
            CodegenResultMapping::Indirect { pointee, .. } => {
                let value = value.expect(
                    "checked MIR control translation requires an established mapping or value",
                );

                let destination = self
                    .function
                    .get_first_param()
                    .and_then(pointer_value)
                    .expect(
                        "checked MIR control translation requires an established mapping or value",
                    );

                let source_type = self.operand_type(value);
                let value = self.operand(value)?;
                let value = self.convert(value, source_type, *pointee)?;

                self.store_value(destination, value, *pointee)?;
                self.clear_moved_reports()?;
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
            CodegenResultMapping::Direct { ty, coercion, .. } => {
                let value = match coercion {
                    Some(coercion) => self.encode_abi_result(value, *ty, coercion)?,
                    None => self.abi_register_value(value, *ty)?,
                };

                llvm(self.builder.build_return(Some(&value)))?;
            }
            CodegenResultMapping::Indirect { pointee, .. } => {
                let destination = self
                    .function
                    .get_first_param()
                    .and_then(pointer_value)
                    .expect(
                        "checked MIR control translation requires an established mapping or value",
                    );

                self.store_value(destination, value, *pointee)?;
                llvm(self.builder.build_return(None))?;
            }
            CodegenResultMapping::Void => {
                panic!("checked MIR control translation violated an established compiler contract");
            }
        }

        Ok(())
    }
}
