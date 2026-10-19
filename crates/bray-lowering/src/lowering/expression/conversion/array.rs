use bray_bound_tree::{BoundExpressionId, SelectedConversion};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirBlockKind, MirEdge, MirOperand, MirOperationKind, MirPlace,
    MirProjection, MirProjectionKind, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind,
};

use crate::lowering::{LoweringError, lowerer::Lowerer};

impl Lowerer<'_> {
    pub(super) fn convert_array_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        operand: MirOperand,
        conversion: &SelectedConversion,
        child: &SelectedConversion,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let input = self.materialize_synthetic_place(
            current,
            Self::retained_source(source),
            operand,
            conversion.source_type(),
        )?;

        let length = self.sequence_length(current, source, &input)?;
        let integer = self.builder.operand_type(&length);
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let zero = crate::operand::integer_constant(self.input.semantic_values(), integer, 0)?;
        let one = crate::operand::integer_constant(self.input.semantic_values(), integer, 1)?;

        let length = self.materialize_synthetic_place(
            current,
            Self::retained_source(source),
            length,
            integer,
        )?;

        let next = self.materialize_synthetic_place(
            current,
            Self::retained_source(source),
            Self::retained_operand(&zero),
            integer,
        )?;

        let checked = child.invokes_source();

        let initialized = if checked {
            self.materialize_synthetic_place(
                current,
                Self::retained_source(source),
                Self::retained_operand(&zero),
                integer,
            )?
        } else {
            Self::retained_place(&next)
        };

        let output = self.builder.push_storage(
            Self::retained_source(source),
            MirStorageKind::Temporary,
            conversion.target_type(),
        )?;

        let output = MirPlace::new(output, [], conversion.target_type());

        if checked {
            self.track_array_input(
                current,
                source,
                Self::retained_place(&input),
                child.source_type(),
                [
                    MirOperand::Copy(Self::retained_place(&next)),
                    MirOperand::Copy(Self::retained_place(&length)),
                ],
            )?;

            self.track_array_input(
                current,
                source,
                Self::retained_place(&output),
                child.target_type(),
                [zero, MirOperand::Copy(Self::retained_place(&initialized))],
            )?;
        }

        let condition = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let body = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let completed = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(condition, [])),
        )?;

        let nonempty = self.push_typed_value_operation(
            expression,
            condition,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::LessThan,
                left: MirOperand::Copy(Self::retained_place(&next)),
                right: MirOperand::Copy(length),
            },
            boolean,
        )?;

        self.set_terminator(
            condition,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: nonempty,
                then_edge: MirEdge::new(body, []),
                else_edge: MirEdge::new(completed, []),
            },
        )?;

        let element = MirPlace::new(
            input.storage(),
            [MirProjection::new(
                MirProjectionKind::Index(MirOperand::Copy(Self::retained_place(&next))),
                input.ty(),
                child.source_type(),
            )],
            child.source_type(),
        );

        let element = self.materialize_owned_input(
            body,
            source,
            MirOperand::Move(element),
            child.source_type(),
        )?;

        if checked {
            let advanced = self.push_typed_value_operation(
                expression,
                body,
                Self::retained_source(source),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::Add,
                    left: MirOperand::Copy(Self::retained_place(&next)),
                    right: Self::retained_operand(&one),
                },
                integer,
            )?;

            self.push_operation(
                body,
                Self::retained_source(source),
                MirOperationKind::Store {
                    kind: MirStoreKind::Assign,
                    destination: next,
                    value: advanced,
                },
                None,
            )?;
        }

        let (continued, converted) = self.convert_operand(
            expression,
            body,
            Self::retained_source(source),
            element,
            child,
        )?;

        let destination = MirPlace::new(
            output.storage(),
            [MirProjection::new(
                MirProjectionKind::Index(MirOperand::Copy(Self::retained_place(&initialized))),
                output.ty(),
                child.target_type(),
            )],
            child.target_type(),
        );

        self.push_operation(
            continued,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination,
                value: converted,
            },
            None,
        )?;

        let advanced = self.push_typed_value_operation(
            expression,
            continued,
            Self::retained_source(source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Add,
                left: MirOperand::Copy(Self::retained_place(&initialized)),
                right: one,
            },
            integer,
        )?;

        self.push_operation(
            continued,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: initialized,
                value: advanced,
            },
            None,
        )?;

        self.set_terminator(
            continued,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(condition, [])),
        )?;

        Ok((completed, MirOperand::Move(output)))
    }
}
