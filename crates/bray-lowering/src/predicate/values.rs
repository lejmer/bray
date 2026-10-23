use bray_bound_tree::{CheckedTemplateOperation, ConstructionInputId, ConstructionTarget};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirConstruction, MirConstructionInput,
    MirFieldReference, MirNumericConversionKind, MirOperand, MirOperationKind, MirPlace,
    MirProjectionKind, MirStorageKind, MirStoreKind,
};
use bray_symbols::{AnySymbolId, TypeData, TypeId};

use super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn convert(
        &mut self,
        block: MirBlockId,
        operand: MirOperand,
        target: TypeId,
    ) -> Result<MirOperand, C::Error> {
        let source = self.builder.operand_type(&operand);

        if source == target {
            return Ok(operand);
        }

        let source_data = self.context.semantic_values().type_data(source);
        let target_data = self.context.semantic_values().type_data(target);

        match (source_data.as_ref(), target_data.as_ref()) {
            (
                _,
                TypeData::Borrow {
                    kind,
                    target: pointee,
                },
            ) if !matches!(source_data.as_ref(), TypeData::Borrow { .. }) || source == *pointee => {
                let operand = self.convert(block, operand, *pointee)?;
                let place = self.place(block, operand)?;

                self.value(
                    block,
                    MirOperationKind::Borrow { kind: *kind, place },
                    target,
                )
            }
            (
                TypeData::Borrow { .. },
                TypeData::Borrow {
                    kind,
                    target: pointee,
                },
            ) => {
                let place = self.observed_place(block, operand)?;

                assert_eq!(
                    place.ty(),
                    *pointee,
                    "checked reborrow retains its target type: source {source_data:?}, target {target_data:?}, observed {:?}, expected {:?}, owner {:?}",
                    self.context.semantic_values().type_data(place.ty()),
                    self.context.semantic_values().type_data(*pointee),
                    self.source
                );

                self.value(
                    block,
                    MirOperationKind::Borrow { kind: *kind, place },
                    target,
                )
            }
            (TypeData::Borrow { .. }, _) => {
                let place = self.observed_place(block, operand)?;

                self.convert(block, MirOperand::Copy(place), target)
            }
            (_, TypeData::Nullable(element)) if *element == source => self.value(
                block,
                MirOperationKind::Aggregate(MirAggregate::new(
                    MirAggregateKind::NullablePresent,
                    [operand],
                )),
                target,
            ),
            _ if self.integer_type_shape(source).is_some()
                && self.integer_type_shape(target).is_some() =>
            {
                self.value(
                    block,
                    MirOperationKind::NumericConversion {
                        kind: MirNumericConversionKind::Truncate,
                        operand,
                    },
                    target,
                )
            }
            (TypeData::Callable(_), TypeData::Callable(_)) => self.value(
                block,
                MirOperationKind::Convert {
                    operand,
                    conversion: bray_bound_tree::SelectedConversion::new(
                        source,
                        target,
                        bray_bound_tree::ConversionTarget::CallableContract,
                    ),
                },
                target,
            ),
            (TypeData::Tuple(elements), TypeData::Named { definition, .. })
                if self
                    .context
                    .representation_role(*definition)
                    .and_then(|role| role.complex_component())
                    .is_some() =>
            {
                let component = self
                    .context
                    .representation_role(*definition)
                    .and_then(|role| role.complex_component())
                    .expect("checked complex conversion has its real component");

                let component = self.context.representation_type(component)?;

                assert!(
                    elements.len() == 2 && elements.iter().all(|ty| *ty == component),
                    "checked tuple conversion has already converted both complex components"
                );

                let children = elements
                    .iter()
                    .map(|ty| {
                        bray_bound_tree::SelectedConversion::new(
                            *ty,
                            component,
                            bray_bound_tree::ConversionTarget::Identity,
                        )
                    })
                    .collect();

                self.value(
                    block,
                    MirOperationKind::Convert {
                        operand,
                        conversion: bray_bound_tree::SelectedConversion::new(
                            source,
                            target,
                            bray_bound_tree::ConversionTarget::Composite(children),
                        ),
                    },
                    target,
                )
            }
            (
                TypeData::Named {
                    definition: source_definition,
                    ..
                },
                TypeData::Named {
                    definition: target_definition,
                    ..
                },
            ) if self
                .context
                .representation_role(*source_definition)
                .and_then(|role| role.numeric_kind())
                .is_some()
                && self
                    .context
                    .representation_role(*target_definition)
                    .and_then(|role| role.numeric_kind())
                    .is_some() =>
            {
                self.value(
                    block,
                    MirOperationKind::NumericConversion {
                        kind: MirNumericConversionKind::Truncate,
                        operand,
                    },
                    target,
                )
            }
            _ => panic!(
                "checked predicate conversion requires a supported semantic conversion: {source:?} {source_data:?} -> {target:?} {target_data:?}"
            ),
        }
    }

    pub(super) fn observe(
        &mut self,
        block: MirBlockId,
        operand: MirOperand,
    ) -> Result<MirOperand, C::Error> {
        if matches!(
            self.context
                .semantic_values()
                .type_data(self.builder.operand_type(&operand))
                .as_ref(),
            TypeData::Borrow { .. }
        ) {
            return Ok(MirOperand::Copy(self.observed_place(block, operand)?));
        }

        Ok(operand)
    }

    pub(super) fn place(
        &mut self,
        block: MirBlockId,
        value: MirOperand,
    ) -> Result<MirPlace, C::Error> {
        if let MirOperand::Copy(place) | MirOperand::Move(place) = value {
            return Ok(place);
        }

        let ty = self.builder.operand_type(&value);

        let storage = self
            .builder
            .push_storage(self.source.clone(), MirStorageKind::Temporary, ty)
            .map_err(capacity)?;

        self.private_homes.insert(storage);

        let place = MirPlace::new(storage, [], ty);

        self.store(block, MirStoreKind::Initialize, place.clone(), value)?;

        Ok(place)
    }

    pub(super) fn observed_place(
        &mut self,
        block: MirBlockId,
        value: MirOperand,
    ) -> Result<MirPlace, C::Error> {
        let mut place = self.place(block, value)?;

        loop {
            let data = self.context.semantic_values().type_data(place.ty());

            let TypeData::Borrow { target, .. } = data.as_ref() else {
                return Ok(place);
            };

            place = place.project(MirProjectionKind::Dereference, *target);
        }
    }

    pub(super) fn structural(
        &mut self,
        mut block: MirBlockId,
        operation: &CheckedTemplateOperation,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let value = match operation {
            CheckedTemplateOperation::Borrow { kind, operand } => {
                let (continued, value) = self.node(block, *operand)?;

                block = continued;

                let place = self.observed_place(block, value)?;

                self.value(block, MirOperationKind::Borrow { kind: *kind, place }, ty)?
            }
            CheckedTemplateOperation::Tuple(elements)
            | CheckedTemplateOperation::Array(elements) => {
                let mut values = Vec::with_capacity(elements.len());

                for element in elements.iter() {
                    let (continued, value) = self.node(block, *element)?;

                    block = continued;
                    values.push(value);
                }

                let kind = if matches!(operation, CheckedTemplateOperation::Tuple(_)) {
                    MirAggregateKind::Tuple
                } else {
                    MirAggregateKind::Array
                };

                self.value(
                    block,
                    MirOperationKind::Aggregate(MirAggregate::new(kind, values)),
                    ty,
                )?
            }
            CheckedTemplateOperation::Construct { variant, fields } => {
                let target = match variant {
                    Some(key) => match self.context.predicate_symbol(key)? {
                        AnySymbolId::UnionVariant(variant) => {
                            ConstructionTarget::UnionVariant(variant)
                        }
                        _ => panic!("checked union construction retains its active variant"),
                    },
                    None => match self.context.semantic_values().type_data(ty).as_ref() {
                        TypeData::Named {
                            definition: bray_symbols::NamedTypeSymbolId::Struct(definition),
                            ..
                        } => ConstructionTarget::Struct(*definition),
                        _ => panic!("checked struct construction retains its named type"),
                    },
                };

                let mut inputs = Vec::with_capacity(fields.len());

                for field in fields.iter() {
                    let id = match self.context.predicate_symbol(field.field())? {
                        AnySymbolId::StructField(field) => ConstructionInputId::StructField(field),
                        AnySymbolId::UnionPayloadField(field) => {
                            ConstructionInputId::UnionPayloadField(field)
                        }
                        _ => {
                            panic!("checked aggregate construction retains its field declarations")
                        }
                    };

                    let (continued, value) = self.node(block, *field.value())?;

                    block = continued;

                    inputs.push(MirConstructionInput::new(
                        id,
                        self.context.predicate_input_ordinal(id)?,
                        value,
                    ));
                }

                self.value(
                    block,
                    MirOperationKind::Construct(MirConstruction::new(target, inputs)),
                    ty,
                )?
            }
            CheckedTemplateOperation::Project { subject, member } => {
                let (continued, value) = self.node(block, *subject)?;

                block = continued;

                let member = match self.context.predicate_symbol(member)? {
                    AnySymbolId::StructField(field) => MirFieldReference::Struct(field),
                    AnySymbolId::UnionPayloadField(field) => MirFieldReference::UnionPayload(field),
                    symbol => panic!("checked value projection retains a field: {symbol:?}"),
                };

                let place = self.observed_place(block, value)?;

                MirOperand::Copy(place.project(MirProjectionKind::Field(member), ty))
            }
            CheckedTemplateOperation::NullableValue { subject } => {
                let (continued, value) = self.node(block, *subject)?;

                block = continued;

                let place = self.observed_place(block, value)?;

                MirOperand::Copy(place.project(MirProjectionKind::NullableValue, ty))
            }
            CheckedTemplateOperation::TupleElement { subject, index } => {
                let (continued, value) = self.node(block, *subject)?;

                block = continued;

                let place = self.observed_place(block, value)?;

                MirOperand::Copy(place.project(MirProjectionKind::TupleField(index.raw()), ty))
            }
            CheckedTemplateOperation::Index {
                subject,
                index,
                call: None,
            } => {
                let (continued, subject) = self.node(block, *subject)?;

                let (continued, index) = self.node(continued, *index)?;

                block = continued;

                let place = self.observed_place(block, subject)?;

                MirOperand::Copy(place.project(MirProjectionKind::Index(index), ty))
            }
            CheckedTemplateOperation::Slice {
                subject,
                lower,
                upper,
                call: None,
            } => {
                let (continued, subject) = self.node(block, *subject)?;

                block = continued;

                let mut bounds = Vec::new();

                for bound in [lower, upper] {
                    bounds.push(if let Some(bound) = bound {
                        let (continued, value) = self.node(block, *bound)?;

                        block = continued;

                        Some(value)
                    } else {
                        None
                    });
                }

                let place = self.observed_place(block, subject)?;

                MirOperand::Copy(place.project(
                    MirProjectionKind::Slice {
                        start: bounds.remove(0),
                        end: bounds.remove(0),
                    },
                    ty,
                ))
            }
            CheckedTemplateOperation::VariantTest { subject, variant } => {
                let (continued, subject) = self.node(block, *subject)?;

                block = continued;

                let AnySymbolId::UnionVariant(variant) = self.context.predicate_symbol(variant)?
                else {
                    panic!("checked union test retains its variant");
                };

                return self.variant_test(block, subject, variant);
            }
            CheckedTemplateOperation::Index { call: Some(_), .. }
            | CheckedTemplateOperation::Slice { call: Some(_), .. } => {
                return self.custom_index(block, operation, ty);
            }
            other => {
                unreachable!("structural predicate dispatch received scalar operation {other:?}")
            }
        };

        Ok((block, value))
    }
}
