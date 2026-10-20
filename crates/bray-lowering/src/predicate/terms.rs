use super::core::{PredicateLowerer, PredicateLoweringContext};
use bray_bound_tree::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateConstantUsage,
    CheckedTemplateExecution, CheckedTemplateInput, CheckedTemplateInputKind, CheckedTemplateKind,
    CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirFieldReference, MirOperand, MirOperationKind,
    MirProjectionKind,
};
use bray_symbols::{
    ConstantProjectionKind, ConstantTermData, ConstantTermId, CurrentRunCancellation,
    SymbolOrdinal, TypeData, TypeId,
};

pub(super) fn condition_template<C: PredicateLoweringContext + ?Sized>(
    context: &C,
    condition: ConstantTermId,
    inputs: &[TypeId],
) -> Result<CheckedTemplate, C::Error> {
    let boolean = context.representation_type(RepresentationRole::ScalarBool)?;

    let dependencies = context
        .semantic_values()
        .empty_dependency_contract_template()
        .map_err(crate::SyntheticLoweringError::SemanticValue)?;

    Ok(CheckedTemplate::from_checked_parts(
        CheckedTemplateKind::CallableContract,
        inputs.iter().enumerate().map(|(ordinal, ty)| {
            CheckedTemplateInput::new(
                CheckedTemplateInputKind::Parameter(SymbolOrdinal::new(
                    u32::try_from(ordinal)
                        .expect("checked callable inputs fit their ordinal table"),
                )),
                *ty,
            )
        }),
        [CheckedTemplateNode::new(
            CheckedTemplateOperation::Constant {
                term: condition,
                usage: CheckedTemplateConstantUsage::new(0, 0, 0),
            },
            boolean,
        )],
        [],
        CheckedTemplateNodeId::new(0),
        CheckedTemplateBehavior::new(
            [],
            [],
            [],
            CheckedTemplateExecution::new([], CurrentRunCancellation::NotEntered),
            [],
            dependencies,
            dependencies,
            [],
        ),
    ))
}

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    fn term_arguments(
        &mut self,
        mut block: MirBlockId,
        arguments: &[ConstantTermId],
        ty: TypeId,
    ) -> Result<(MirBlockId, Vec<MirOperand>), C::Error> {
        let mut values = Vec::with_capacity(arguments.len());

        for argument in arguments {
            let (continued, value) = self.term(block, *argument, ty)?;

            block = continued;
            values.push(value);
        }

        Ok((block, values))
    }

    pub(super) fn term(
        &mut self,
        mut block: MirBlockId,
        term: ConstantTermId,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let data = self.context.semantic_values().constant_term_data(term);

        match data.as_ref() {
            ConstantTermData::Typed { term, ty } => {
                self.term(block, *term, self.context.predicate_type(*ty)?)
            }
            ConstantTermData::CallableArgument(ordinal) => Ok((
                block,
                self.inputs
                    [usize::try_from(ordinal.raw()).expect("checked callable ordinal fits host")]
                .as_ref()
                .expect("runtime predicate input is a value")
                .clone(),
            )),
            ConstantTermData::Unary { operation, operand } => {
                let (block, value) = self.term(block, *operand, ty)?;

                let value = self.unary(block, *operation, value, ty)?;

                Ok((block, value))
            }
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => {
                let (block, left) = self.term(block, *left, ty)?;

                if matches!(
                    operation,
                    bray_symbols::ConstantBinaryOperation::LogicalAnd
                        | bray_symbols::ConstantBinaryOperation::LogicalOr
                ) {
                    let conjunction =
                        *operation == bray_symbols::ConstantBinaryOperation::LogicalAnd;

                    return self.branch_values(
                        block,
                        left,
                        self.boolean,
                        |lowerer, block| {
                            if conjunction {
                                lowerer.term(block, *right, ty)
                            } else {
                                Ok((block, lowerer.boolean_value(true)))
                            }
                        },
                        |lowerer, block| {
                            if conjunction {
                                Ok((block, lowerer.boolean_value(false)))
                            } else {
                                lowerer.term(block, *right, ty)
                            }
                        },
                    );
                }

                let (block, right) = self.term(block, *right, ty)?;

                self.binary(block, *operation, left, right, ty)
            }
            ConstantTermData::Conversion { operand, target } => {
                let (block, value) = self.term(block, *operand, ty)?;

                let value = self.convert(block, value, self.context.predicate_type(*target)?)?;

                Ok((block, value))
            }
            ConstantTermData::PredicateCall {
                predicate,
                arguments,
            } => {
                let (block, values) = self.term_arguments(block, arguments, ty)?;

                let call = self.context.predicate_application(*predicate, values)?;

                self.call(block, call)
            }
            ConstantTermData::Call {
                callable,
                arguments,
                selected_implementation,
            } => {
                let (block, values) = self.term_arguments(block, arguments, ty)?;

                let callable = self
                    .context
                    .semantic_values()
                    .callable_instance_data(*callable);

                let call = self.context.predicate_instance_call(
                    *callable,
                    *selected_implementation,
                    values,
                )?;

                self.call(block, call)
            }
            ConstantTermData::Tuple(elements) | ConstantTermData::Array(elements) => {
                let (block, values) = self.term_arguments(block, elements, ty)?;

                let kind = if matches!(data.as_ref(), ConstantTermData::Tuple(_)) {
                    MirAggregateKind::Tuple
                } else {
                    MirAggregateKind::Array
                };

                let value = self.value(
                    block,
                    MirOperationKind::Aggregate(MirAggregate::new(kind, values)),
                    ty,
                )?;

                Ok((block, value))
            }
            ConstantTermData::NullablePresent(term) => {
                let (block, value) = self.term(block, *term, ty)?;

                let value = self.value(
                    block,
                    MirOperationKind::Aggregate(MirAggregate::new(
                        MirAggregateKind::NullablePresent,
                        [value],
                    )),
                    ty,
                )?;

                Ok((block, value))
            }
            ConstantTermData::VariantTest { subject, variant } => {
                let (block, subject) = self.term(block, *subject, ty)?;

                self.variant_test(block, subject, *variant)
            }
            ConstantTermData::Projection(projection) => {
                let (continued, value) = self.term(block, projection.subject(), ty)?;

                block = continued;

                let projection = match projection.kind() {
                    ConstantProjectionKind::TupleElement(ordinal) => {
                        MirProjectionKind::TupleField(ordinal.raw())
                    }
                    ConstantProjectionKind::ProductField(field) => {
                        MirProjectionKind::Field(MirFieldReference::Struct(field))
                    }
                    ConstantProjectionKind::UnionPayloadField(field) => {
                        MirProjectionKind::Field(MirFieldReference::UnionPayload(field))
                    }
                    ConstantProjectionKind::NullableValue => MirProjectionKind::NullableValue,
                    ConstantProjectionKind::ArrayElementOrdinal(ordinal) => {
                        MirProjectionKind::ElementFromStart(ordinal.raw())
                    }
                    ConstantProjectionKind::ArrayElement(index) => {
                        let (continued, index) = self.term(
                            block,
                            index,
                            self.context
                                .representation_type(RepresentationRole::ScalarUsize)?,
                        )?;

                        block = continued;

                        MirProjectionKind::Index(index)
                    }
                    ConstantProjectionKind::ArraySlice { lower, upper } => {
                        let mut bounds = Vec::new();

                        for bound in [lower, upper] {
                            bounds.push(if let Some(bound) = bound {
                                let (continued, value) = self.term(
                                    block,
                                    bound,
                                    self.context
                                        .representation_type(RepresentationRole::ScalarUsize)?,
                                )?;

                                block = continued;

                                Some(value)
                            } else {
                                None
                            });
                        }

                        MirProjectionKind::Slice {
                            start: bounds.remove(0),
                            end: bounds.remove(0),
                        }
                    }
                };

                let place = self.observed_place(block, value)?;
                let data = self.context.semantic_values().type_data(place.ty());

                let ty = match (&projection, data.as_ref()) {
                    (MirProjectionKind::Field(field), _) => {
                        self.context.predicate_field_type(place.ty(), *field)?
                    }
                    (MirProjectionKind::TupleField(ordinal), TypeData::Tuple(elements)) => {
                        elements
                            [usize::try_from(*ordinal).expect("checked tuple ordinal fits host")]
                    }
                    (MirProjectionKind::NullableValue, TypeData::Nullable(element))
                    | (
                        MirProjectionKind::Index(_) | MirProjectionKind::ElementFromStart(_),
                        TypeData::Array { element, .. } | TypeData::Slice(element),
                    ) => *element,
                    (
                        MirProjectionKind::Slice { .. },
                        TypeData::Array { element, .. } | TypeData::Slice(element),
                    ) => self
                        .context
                        .semantic_values()
                        .intern_type(TypeData::Slice(*element))
                        .map_err(crate::SyntheticLoweringError::SemanticValue)?,
                    _ => panic!(
                        "checked symbolic projection retains a matching subject type: {projection:?}, {data:?}"
                    ),
                };

                Ok((block, MirOperand::Copy(place.project(projection, ty))))
            }
            _ => {
                let value = self.context.predicate_constant(term)?;

                let ty = self
                    .context
                    .semantic_values()
                    .constant_value_data(value)
                    .ty();

                Ok((block, MirOperand::Constant { value, ty }))
            }
        }
    }
}
