use super::core::UnitTranslator;
use super::support::{
    aggregate_element, extract_value, int_value, llvm, pointer_value, result_type, u128_words,
};
use bray_codegen::{CodegenFailure, CodegenResultMapping, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::{MirBlockId, MirEdge, MirOperand, MirPlace, MirTerminatorKind, PatternPredicate};
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::{FloatPredicate, IntPredicate};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_terminator(
        &mut self,
        block: MirBlockId,
        terminator: &MirTerminatorKind,
    ) -> Result<(), CodegenFailure> {
        // Keep this exhaustive so every MIR terminator requires an explicit translation.
        match terminator {
            MirTerminatorKind::Goto(edge) => {
                self.add_edge_arguments(edge)?;

                llvm(
                    self.builder
                        .build_unconditional_branch(self.block(edge.target())?),
                )?;
            }
            MirTerminatorKind::BeginCleanup(cleanup)
            | MirTerminatorKind::ContinueCleanup(cleanup) => {
                let edge = cleanup.edge();

                self.add_edge_arguments(edge)?;

                llvm(
                    self.builder
                        .build_unconditional_branch(self.block(edge.target())?),
                )?;
            }
            MirTerminatorKind::Branch {
                condition,
                then_edge,
                else_edge,
            } => {
                let condition = int_value(self.operand(condition)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.add_edge_arguments(then_edge)?;
                self.add_edge_arguments(else_edge)?;

                llvm(self.builder.build_conditional_branch(
                    condition,
                    self.block(then_edge.target())?,
                    self.block(else_edge.target())?,
                ))?;
            }
            MirTerminatorKind::Switch {
                discriminant,
                cases,
                otherwise,
            } => {
                let discriminant = int_value(self.operand(discriminant)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.add_edge_arguments(otherwise)?;

                let mut llvm_cases = Vec::with_capacity(cases.len());

                for case in cases.iter() {
                    self.add_edge_arguments(case.edge())?;

                    let value = int_value(self.constant(case.value())?)
                        .filter(|value| value.get_type() == discriminant.get_type())
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    llvm_cases.push((value, self.block(case.edge().target())?));
                }

                llvm(self.builder.build_switch(
                    discriminant,
                    self.block(otherwise.target())?,
                    &llvm_cases,
                ))?;
            }
            MirTerminatorKind::Return(value) => {
                self.translate_return(value.as_ref())?;
            }
            MirTerminatorKind::Unreachable => {
                llvm(self.builder.build_unreachable())?;
            }
            MirTerminatorKind::Panic { cleanup, .. }
            | MirTerminatorKind::CancelCurrentRun { cleanup } => {
                self.add_edge_arguments(cleanup.edge())?;

                llvm(
                    self.builder
                        .build_unconditional_branch(self.block(cleanup.edge().target())?),
                )?;
            }
            MirTerminatorKind::PatternBranch {
                subject,
                predicate,
                matched,
                unmatched,
            } => {
                let subject_type = self.operand_type(subject)?;
                let subject = self.operand(subject)?;

                let condition =
                    self.translate_pattern_predicate(block, subject, subject_type, *predicate)?;

                self.add_edge_arguments(matched)?;
                self.add_edge_arguments(unmatched)?;

                llvm(self.builder.build_conditional_branch(
                    condition,
                    self.block(matched.target())?,
                    self.block(unmatched.target())?,
                ))?;
            }
            MirTerminatorKind::Iterate {
                cursor,
                next,
                element_type,
                item,
                exhausted,
            } => {
                self.translate_iteration(cursor, *next, *element_type, *item, exhausted)?;
            }
            MirTerminatorKind::Suspend {
                resume,
                cancellation,
                registration,
                wake,
                ..
            } => {
                let wake = self.runtime_function_pointer(*wake)?;
                let outcome = self.invoke_runtime(*registration, &[wake.into()])?;

                self.add_edge_arguments(resume)?;
                self.add_edge_arguments(cancellation.edge())?;

                match outcome.and_then(int_value) {
                    Some(cancelled) => llvm(self.builder.build_conditional_branch(
                        cancelled,
                        self.block(cancellation.edge().target())?,
                        self.block(resume.target())?,
                    ))?,
                    None => llvm(
                        self.builder
                            .build_unconditional_branch(self.block(resume.target())?),
                    )?,
                };
            }
            MirTerminatorKind::ForwardRunResult { result, edges } => {
                let result = self.operand(result)?;
                let tag = self.run_result_tag(result)?;

                self.add_edge_arguments(edges.completed())?;
                self.add_edge_arguments(edges.panicked().edge())?;
                self.add_edge_arguments(edges.cancelled().edge())?;

                let panicked = tag.get_type().const_int(1, false);
                let cancelled = tag.get_type().const_int(2, false);

                let cases = [
                    (panicked, self.block(edges.panicked().edge().target())?),
                    (cancelled, self.block(edges.cancelled().edge().target())?),
                ];

                llvm(self.builder.build_switch(
                    tag,
                    self.block(edges.completed().target())?,
                    &cases,
                ))?;
            }
        }

        Ok(())
    }

    pub(super) fn translate_iteration(
        &mut self,
        cursor: &MirPlace,
        next: bray_ir::MirCallableReference,
        element_type: bray_symbols::TypeId,
        item: MirBlockId,
        exhausted: &MirEdge,
    ) -> Result<(), CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .callable(self.instance.key(), next)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let symbol = self
            .request
            .mappings()
            .instance_symbol(mapping.instance())
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
        let element = self.project_value(result, element_type)?;

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

        let source = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        phi.add_incoming(&[(&element, source)]);
        self.add_edge_arguments(exhausted)?;

        llvm(self.builder.build_conditional_branch(
            present,
            self.block(item)?,
            self.block(exhausted.target())?,
        ))?;

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

    pub(super) fn run_result_tag(
        &self,
        result: BasicValueEnum<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        int_value(extract_value(&self.builder, result, 0)?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn translate_pattern_predicate(
        &mut self,
        block: MirBlockId,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        predicate: PatternPredicate,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        match predicate {
            PatternPredicate::Literal(_) => {
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
            PatternPredicate::Constant(term) => {
                let value = self
                    .request
                    .mappings()
                    .constant_term(self.instance.key(), term)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let value = self.constant(value)?;

                self.equal_values(subject, value)
            }
            PatternPredicate::NullableAbsent | PatternPredicate::NullablePresent => {
                let present = self.nullable_present(subject, subject_type)?;

                if predicate == PatternPredicate::NullablePresent {
                    Ok(present)
                } else {
                    llvm(self.builder.build_not(present, "pattern.nullable.absent"))
                }
            }
            PatternPredicate::ActiveUnionVariant(variant) => {
                self.active_union_variant(subject, subject_type, variant)
            }
            PatternPredicate::ProductShape(_)
            | PatternPredicate::TupleShape(_)
            | PatternPredicate::ArrayShape(_)
            | PatternPredicate::OwnedTarget => {
                Ok(self.types.context().bool_type().const_int(1, false))
            }
        }
    }

    pub(super) fn equal_values(
        &self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
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
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
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

    pub(super) fn nullable_present(
        &self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match (mapping.kind(), subject) {
            (CodegenTypeKind::Pointer { .. }, BasicValueEnum::PointerValue(pointer)) => {
                llvm(self.builder.build_is_not_null(pointer, "nullable.present"))
            }
            (CodegenTypeKind::Aggregate(fields), subject) if fields.len() > 1 => {
                let tag = extract_value(&self.builder, subject, aggregate_element(fields, 0)?)?;
                let tag = int_value(tag).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_int_compare(
                    IntPredicate::NE,
                    tag,
                    tag.get_type().const_zero(),
                    "nullable.present",
                ))
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn active_union_variant(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag_type = *tag;

        let expected = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .map(bray_codegen::CodegenUnionVariantLayout::tag)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let storage = llvm(
            self.builder
                .build_alloca(subject.get_type(), "pattern.union.subject"),
        )?;

        llvm(self.builder.build_store(storage, subject))?;

        let tag = llvm(self.builder.build_load(
            self.types.map(tag_type)?,
            storage,
            "pattern.union.tag",
        ))?;

        let tag = int_value(tag).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let expected = tag
            .get_type()
            .const_int_arbitrary_precision(&u128_words(expected));

        llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            tag,
            expected,
            "pattern.union.active",
        ))
    }

    pub(super) fn translate_return(
        &mut self,
        value: Option<&MirOperand>,
    ) -> Result<(), CodegenFailure> {
        match self.signature.result() {
            CodegenResultMapping::Void => {
                if value.is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                llvm(self.builder.build_return(None))?;
            }
            CodegenResultMapping::Direct { .. } => {
                let value = value.ok_or(CodegenFailure::GeneratedModuleInvariant)?;
                let value = self.operand(value)?;

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
                llvm(self.builder.build_return(None))?;
            }
        }

        Ok(())
    }

    pub(super) fn add_edge_arguments(&mut self, edge: &MirEdge) -> Result<(), CodegenFailure> {
        let target = self
            .unit
            .block(edge.target())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if target.parameters().len() != edge.arguments().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let Some(source) = self.builder.get_insert_block() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        for (parameter, argument) in target.parameters().iter().zip(edge.arguments()) {
            let phi = self
                .phis
                .get(parameter)
                .copied()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let value = self.operand(argument)?;

            phi.add_incoming(&[(&value, source)]);
        }

        Ok(())
    }
}
