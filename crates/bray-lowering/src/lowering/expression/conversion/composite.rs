use bray_bound_tree::{BoundExpressionId, SelectedConversion};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirBlockKind, MirEdge, MirImmediateValue,
    MirOperand, MirOperationKind, MirPatternPredicate, MirPlace, MirProjection, MirProjectionKind,
    MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::TypeData;

use crate::lowering::{LoweringError, lowerer::Lowerer};

impl Lowerer<'_> {
    pub(super) fn convert_composite_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        operand: MirOperand,
        conversion: &SelectedConversion,
        children: &[SelectedConversion],
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        match self
            .input
            .semantic_values()
            .type_data(conversion.source_type())
            .as_ref()
        {
            TypeData::Tuple(_) => self
                .convert_tuple_operand(expression, current, source, operand, conversion, children),
            TypeData::Array { .. } => {
                let [child] = children else {
                    panic!("array conversion retains its single element plan");
                };

                self.convert_array_operand(expression, current, source, operand, conversion, child)
            }
            TypeData::Nullable(_) => {
                let [child] = children else {
                    panic!("nullable conversion retains its single payload plan");
                };

                self.convert_nullable_operand(
                    expression, current, source, operand, conversion, child,
                )
            }
            _ => panic!("source-calling composite conversion retains its checked aggregate shape"),
        }
    }

    fn convert_tuple_operand(
        &mut self,
        expression: BoundExpressionId,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        operand: MirOperand,
        conversion: &SelectedConversion,
        children: &[SelectedConversion],
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        // Decomposition is infallible. Establish every child owner before the first
        // call, so ordinary input cleanup owns all unconverted components.
        let place = self.materialize_synthetic_place(
            block,
            Self::retained_source(source),
            operand,
            conversion.source_type(),
        )?;

        let mut inputs = Vec::with_capacity(children.len());

        for (ordinal, child) in children.iter().enumerate() {
            let component = MirPlace::new(
                place.storage(),
                [MirProjection::new(
                    MirProjectionKind::TupleField(
                        u32::try_from(ordinal)
                            .expect("tuple component ordinal fits its checked type"),
                    ),
                    conversion.source_type(),
                    child.source_type(),
                )],
                child.source_type(),
            );

            inputs.push(self.materialize_owned_input(
                block,
                source,
                MirOperand::Move(component),
                child.source_type(),
            )?);
        }

        let mut output = Vec::with_capacity(children.len());

        for (input, child) in inputs.into_iter().zip(children) {
            let (continued, converted) = self.convert_operand(
                expression,
                block,
                Self::retained_source(source),
                input,
                child,
            )?;

            block = continued;

            output.push(self.materialize_owned_input(
                block,
                source,
                converted,
                child.target_type(),
            )?);
        }

        let tuple = self.input.semantic_values().intern_type(TypeData::tuple(
            children.iter().map(SelectedConversion::target_type),
        ))?;

        let value = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(source),
            MirOperationKind::Aggregate(MirAggregate::new(MirAggregateKind::Tuple, output)),
            tuple,
        )?;

        if tuple == conversion.target_type() {
            return Ok((block, value));
        }

        let conversion = SelectedConversion::new(
            tuple,
            conversion.target_type(),
            bray_bound_tree::ConversionTarget::Composite(
                children
                    .iter()
                    .map(|child| {
                        SelectedConversion::new(
                            child.target_type(),
                            child.target_type(),
                            bray_bound_tree::ConversionTarget::Identity,
                        )
                    })
                    .collect(),
            ),
        );

        self.convert_operand(
            expression,
            block,
            Self::retained_source(source),
            value,
            &conversion,
        )
    }

    fn convert_nullable_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        operand: MirOperand,
        conversion: &SelectedConversion,
        child: &SelectedConversion,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let stored =
            self.materialize_owned_input(current, source, operand, conversion.source_type())?;

        let MirOperand::Move(place) = stored else {
            panic!("nullable conversion owns its addressable input");
        };

        let matched = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let absent = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let join = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let result = self.builder.push_block_parameter(
            join,
            Self::retained_source(source),
            conversion.target_type(),
        )?;

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::PatternBranch {
                subject: MirOperand::Copy(Self::retained_place(&place)),
                predicate: MirPatternPredicate::NullablePresent,
                matched: MirEdge::new(matched, []),
                unmatched: MirEdge::new(absent, []),
            },
        )?;

        let payload = MirPlace::new(
            place.storage(),
            [MirProjection::new(
                MirProjectionKind::NullableValue,
                conversion.source_type(),
                child.source_type(),
            )],
            child.source_type(),
        );

        let payload = self.materialize_owned_input(
            matched,
            source,
            MirOperand::Move(payload),
            child.source_type(),
        )?;

        self.set_storage_initialized(matched, source, &place, false)?;

        let (matched, converted) = self.convert_operand(
            expression,
            matched,
            Self::retained_source(source),
            payload,
            child,
        )?;

        let converted = self.push_nullable_present(
            expression,
            matched,
            Self::retained_source(source),
            converted,
            conversion.target_type(),
        )?;

        self.set_terminator(
            matched,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(join, [converted])),
        )?;

        self.set_storage_initialized(absent, source, &place, false)?;

        self.set_terminator(
            absent,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(
                join,
                [Self::immediate_operand(
                    conversion.target_type(),
                    MirImmediateValue::NullableAbsent,
                )],
            )),
        )?;

        Ok((join, MirOperand::Value(result)))
    }
}
