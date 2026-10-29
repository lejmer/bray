use bray_bound_tree::{BoundExpressionId, CheckedMemoryOperationKind, MemoryLayoutQueryKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirBlockKind, MirEdge, MirMemoryOperation, MirOperand,
    MirOperationKind, MirPanicCause, MirPlace, MirSourceAnchor, MirTerminatorKind,
    MirUnaryOperator,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use crate::lowering::{LoweringError, expression::CallOutcome, lowerer::Lowerer};

impl Lowerer<'_> {
    pub(in crate::lowering::expression::control) fn push_generator_element(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        destination: MirPlace,
        element: TypeId,
        value: MirOperand,
    ) -> Result<MirBlockId, LoweringError> {
        let retained = self.input_temporaries.len();
        let value = self.materialize_owned_input(current, source, value, element)?;
        let integer = self.representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let buffer = self.generator_buffer_borrow(expression, current, source, destination)?;

        let mut fields = Vec::new();

        for kind in [
            CheckedMemoryOperationKind::RawBufferInitializedCount,
            CheckedMemoryOperationKind::RawBufferCapacity,
        ] {
            let field = self.generator_memory_value(
                expression,
                current,
                source,
                MirMemoryOperation::new(
                    kind,
                    [MirOperand::Copy(Self::retained_place(&buffer))],
                    [buffer.ty()],
                    Some(integer),
                ),
            )?;

            fields.push(self.materialize_synthetic_place(
                current,
                Self::retained_source(source),
                field,
                integer,
            )?);
        }

        let [initialized, capacity]: [MirPlace; 2] = fields
            .try_into()
            .expect("generator growth observes its initialized prefix and capacity");

        let spare = self.push_typed_value_operation(
            expression,
            current,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::LessThan,
                left: MirOperand::Copy(Self::retained_place(&initialized)),
                right: MirOperand::Copy(Self::retained_place(&capacity)),
            },
            boolean,
        )?;

        let ready = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let grow = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: spare,
                then_edge: MirEdge::new(ready, []),
                else_edge: MirEdge::new(grow, []),
            },
        )?;

        let capacity = self.generator_growth_capacity(expression, grow, source, &capacity)?;

        let (grown, capacity) = capacity;

        let grown =
            self.grow_generator_buffer(expression, grown, source, &buffer, element, capacity)?;

        self.set_terminator(
            grown,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(ready, [])),
        )?;

        let buffer_type = buffer.ty();

        self.push_operation(
            ready,
            Self::retained_source(source),
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferPush { element },
                [MirOperand::Copy(buffer), value],
                [buffer_type, element],
                None,
            ))),
            None,
        )?;

        self.input_temporaries.truncate(retained);

        Ok(ready)
    }

    fn generator_growth_capacity(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        capacity: &MirPlace,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let integer = capacity.ty();
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let constant =
            |value| crate::operand::integer_constant(self.input.semantic_values(), integer, value);

        let zero = constant(0)?;
        let one = constant(1)?;
        let four = constant(4)?;

        let maximum = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Unary {
                operator: MirUnaryOperator::BitwiseNot,
                operand: zero,
            },
            integer,
        )?;

        let maximum = self.materialize_synthetic_place(
            block,
            Self::retained_source(source),
            maximum,
            integer,
        )?;

        let exhausted = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Equal,
                left: MirOperand::Copy(Self::retained_place(capacity)),
                right: MirOperand::Copy(Self::retained_place(&maximum)),
            },
            boolean,
        )?;

        let failure = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let available = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: exhausted,
                then_edge: MirEdge::new(failure, []),
                else_edge: MirEdge::new(available, []),
            },
        )?;

        self.generator_layout_failure(
            expression,
            failure,
            source,
            MirPanicCause::MemoryLayoutSizeOverflow,
        )?;

        let small = self.push_typed_value_operation(
            expression,
            available,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::LessThan,
                left: MirOperand::Copy(Self::retained_place(capacity)),
                right: Self::retained_operand(&four),
            },
            boolean,
        )?;

        let larger = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let join = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let result =
            self.builder
                .push_block_parameter(join, Self::retained_source(source), integer)?;

        self.set_terminator(
            available,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: small,
                then_edge: MirEdge::new(join, [four]),
                else_edge: MirEdge::new(larger, []),
            },
        )?;

        let remaining = self.push_typed_value_operation(
            expression,
            larger,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Subtract,
                left: MirOperand::Copy(maximum),
                right: MirOperand::Copy(Self::retained_place(capacity)),
            },
            integer,
        )?;

        let doubles = self.push_typed_value_operation(
            expression,
            larger,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::LessThanOrEqual,
                left: MirOperand::Copy(Self::retained_place(capacity)),
                right: remaining,
            },
            boolean,
        )?;

        let doubled = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let exact = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            larger,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: doubles,
                then_edge: MirEdge::new(doubled, []),
                else_edge: MirEdge::new(exact, []),
            },
        )?;

        for (block, additional) in [
            (doubled, MirOperand::Copy(Self::retained_place(capacity))),
            (exact, one),
        ] {
            let grown = self.push_typed_value_operation(
                expression,
                block,
                Self::retained_source(source),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::Add,
                    left: MirOperand::Copy(Self::retained_place(capacity)),
                    right: additional,
                },
                integer,
            )?;

            self.set_terminator(
                block,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(join, [grown])),
            )?;
        }

        Ok((join, MirOperand::Value(result)))
    }

    fn grow_generator_buffer(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        buffer: &MirPlace,
        element: TypeId,
        capacity: MirOperand,
    ) -> Result<MirBlockId, LoweringError> {
        let integer = self.representation_type(RepresentationRole::ScalarUsize)?;

        let capacity = self.materialize_synthetic_place(
            block,
            Self::retained_source(source),
            capacity,
            integer,
        )?;

        let block = self.check_generator_layout(expression, block, source, element, &capacity)?;

        let mut layout = Vec::new();

        for kind in [
            MemoryLayoutQueryKind::Stride,
            MemoryLayoutQueryKind::Alignment,
        ] {
            layout.push(self.generator_memory_value(
                expression,
                block,
                source,
                MirMemoryOperation::new(
                    CheckedMemoryOperationKind::LayoutQuery { ty: element, kind },
                    [],
                    [],
                    Some(integer),
                ),
            )?);
        }

        let [stride, alignment]: [MirOperand; 2] = layout
            .try_into()
            .expect("allocation retains the checked stride and alignment");

        let bytes = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Multiply,
                left: stride,
                right: MirOperand::Copy(Self::retained_place(&capacity)),
            },
            integer,
        )?;

        let bytes =
            self.materialize_synthetic_place(block, Self::retained_source(source), bytes, integer)?;

        let alignment = self.materialize_synthetic_place(
            block,
            Self::retained_source(source),
            alignment,
            integer,
        )?;

        let TypeData::Borrow { target: owner, .. } =
            *self.input.semantic_values().type_data(buffer.ty())
        else {
            panic!("generator accumulation retains a borrow of its owner");
        };

        let block = self.admit_outgoing_owner(expression, block, source, owner)?;

        let replacement = self.generator_memory_value(
            expression,
            block,
            source,
            MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferAllocate,
                [
                    MirOperand::Copy(capacity),
                    MirOperand::Copy(bytes),
                    MirOperand::Copy(alignment),
                ],
                [integer, integer, integer],
                Some(owner),
            ),
        )?;

        let (block, replacement) = self.finish_typed_call_panic_check(
            expression,
            block,
            source,
            &replacement,
            owner,
            Some(owner),
            CallOutcome::Runtime,
        )?;

        let MirOperand::Move(replacement) =
            self.materialize_owned_input(block, source, replacement, owner)?
        else {
            panic!("generator growth retains its replacement owner until publication");
        };

        let borrowed = self.generator_buffer_borrow(
            expression,
            block,
            source,
            Self::retained_place(&replacement),
        )?;

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferRelocate { element },
                [
                    MirOperand::Copy(Self::retained_place(buffer)),
                    MirOperand::Copy(Self::retained_place(&borrowed)),
                ],
                [buffer.ty(), borrowed.ty()],
                None,
            ))),
            None,
        )?;

        // Relocation has retired the old initialized prefix. Release only its
        // empty backing; no element lifecycle can suspend at this construction step.
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferRelease { element },
                [MirOperand::Copy(Self::retained_place(buffer))],
                [buffer.ty()],
                None,
            ))),
            None,
        )?;

        let unit = self.representation_type(RepresentationRole::Unit)?;
        let completed = self.unit_operand(unit);

        let (block, _) = self.finish_typed_call_panic_check(
            expression,
            block,
            source,
            &completed,
            unit,
            None,
            CallOutcome::Runtime,
        )?;

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferReplace { element },
                [
                    MirOperand::Copy(Self::retained_place(buffer)),
                    MirOperand::Copy(borrowed),
                ],
                [buffer.ty(), buffer.ty()],
                None,
            ))),
            None,
        )?;

        // Replacing transfers the backing into the existing owner. The emptied
        // replacement no longer needs the allowance admitted for its allocation.
        self.set_storage_initialized(block, source, &replacement, false)?;
        self.discharge_outgoing_owner(block, source, owner)?;

        Ok(block)
    }

    fn check_generator_layout(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        element: TypeId,
        capacity: &MirPlace,
    ) -> Result<MirBlockId, LoweringError> {
        let integer = capacity.ty();
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let zero = crate::operand::integer_constant(self.input.semantic_values(), integer, 0)?;
        let one = crate::operand::integer_constant(self.input.semantic_values(), integer, 1)?;

        let status = self.generator_memory_value(
            expression,
            block,
            source,
            MirMemoryOperation::new(
                CheckedMemoryOperationKind::LayoutQuery {
                    ty: element,
                    kind: MemoryLayoutQueryKind::LayoutStatus,
                },
                [MirOperand::Copy(Self::retained_place(capacity))],
                [integer],
                Some(integer),
            ),
        )?;

        let status = self.materialize_synthetic_place(
            block,
            Self::retained_source(source),
            status,
            integer,
        )?;

        let valid = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Equal,
                left: MirOperand::Copy(Self::retained_place(&status)),
                right: zero,
            },
            boolean,
        )?;

        let ready = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let failure = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: valid,
                then_edge: MirEdge::new(ready, []),
                else_edge: MirEdge::new(failure, []),
            },
        )?;

        let overflow = self.push_typed_value_operation(
            expression,
            failure,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Equal,
                left: MirOperand::Copy(status),
                right: one,
            },
            boolean,
        )?;

        let size_failure = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let alignment_failure = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            failure,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: overflow,
                then_edge: MirEdge::new(size_failure, []),
                else_edge: MirEdge::new(alignment_failure, []),
            },
        )?;

        self.generator_layout_failure(
            expression,
            size_failure,
            source,
            MirPanicCause::MemoryLayoutSizeOverflow,
        )?;

        self.generator_layout_failure(
            expression,
            alignment_failure,
            source,
            MirPanicCause::MemoryLayoutUnsupportedAlignment,
        )?;

        Ok(ready)
    }

    fn generator_buffer_borrow(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
    ) -> Result<MirPlace, LoweringError> {
        let kind = BorrowKind::Mutable;

        let ty = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind,
            target: place.ty(),
        })?;

        let borrowed = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Borrow { kind, place },
            ty,
        )?;

        self.materialize_synthetic_place(block, Self::retained_source(source), borrowed, ty)
    }

    fn generator_memory_value(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        memory: MirMemoryOperation,
    ) -> Result<MirOperand, LoweringError> {
        let ty = memory
            .result_type()
            .expect("generator memory observation publishes a value");

        self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Memory(Box::new(memory)),
            ty,
        )
    }

    fn generator_layout_failure(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        cause: MirPanicCause,
    ) -> Result<(), LoweringError> {
        let ty = self.representation_type(RepresentationRole::PanicReport)?;
        let report = self.push_panic_report(expression, block, source, cause, ty)?;

        self.finish_panic_to_active_catch(expression, block, source, report, ty)
    }
}
