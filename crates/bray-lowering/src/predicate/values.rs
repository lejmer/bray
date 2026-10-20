use bray_bound_tree::{CheckedTemplateOperation, ConstructionInputId, ConstructionTarget};
use bray_ir::{MirAggregate, MirAggregateKind, MirBlockId, MirConstruction, MirConstructionInput, MirFieldReference, MirOperand, MirOperationKind, MirPlace, MirProjectionKind, MirStorageKind, MirStoreKind, MirNumericConversionKind};
use bray_symbols::{AnySymbolId, BorrowKind, TypeData, TypeId};

use super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn convert(&mut self, block: MirBlockId, operand: MirOperand, target: TypeId) -> Result<MirOperand, C::Error> {
        let source = self.builder.operand_type(&operand);
        if source == target { return Ok(operand); }
        let source_data = self.context.semantic_values().type_data(source);
        let target_data = self.context.semantic_values().type_data(target);
        match (source_data.as_ref(), target_data.as_ref()) {
            (_, TypeData::Borrow { kind, target: pointee }) if source == *pointee => {
                let place = self.place(block, operand)?;
                self.value(block, MirOperationKind::Borrow { kind: *kind, place }, target)
            }
            (TypeData::Borrow { .. }, TypeData::Borrow { kind, target: pointee }) => {
                let place = self.observed_place(block, operand)?;
                assert_eq!(place.ty(), *pointee, "checked reborrow retains its target type");
                self.value(block, MirOperationKind::Borrow { kind: *kind, place }, target)
            }
            (TypeData::Borrow { .. }, _) => {
                let place = self.observed_place(block, operand)?;
                self.convert(block, MirOperand::Copy(place), target)
            }
            (_, TypeData::Nullable(element)) if *element == source => self.value(block, MirOperationKind::Aggregate(MirAggregate::new(MirAggregateKind::NullablePresent, [operand])), target),
            _ if self.integer_type_shape(source).is_some() && self.integer_type_shape(target).is_some() => self.value(block, MirOperationKind::NumericConversion { kind: MirNumericConversionKind::Truncate, operand }, target),
            (TypeData::Named { definition: source_definition, .. }, TypeData::Named { definition: target_definition, .. })
                if self.context.representation_role(*source_definition).and_then(|role| role.numeric_kind()).is_some()
                    && self.context.representation_role(*target_definition).and_then(|role| role.numeric_kind()).is_some() => self.value(block, MirOperationKind::NumericConversion { kind: MirNumericConversionKind::Truncate, operand }, target),
            _ => panic!("checked predicate conversion requires a supported semantic conversion: {source:?} -> {target:?}"),
        }
    }

    pub(super) fn observe(&mut self, block: MirBlockId, operand: MirOperand) -> Result<MirOperand, C::Error> {
        if matches!(self.context.semantic_values().type_data(self.builder.operand_type(&operand)).as_ref(), TypeData::Borrow { .. }) {
            return Ok(MirOperand::Copy(self.observed_place(block, operand)?));
        }
        Ok(operand)
    }

    pub(super) fn place(&mut self, block: MirBlockId, value: MirOperand) -> Result<MirPlace, C::Error> {
        if let MirOperand::Copy(place) | MirOperand::Move(place) = value { return Ok(place); }
        let ty = self.builder.operand_type(&value);
        let storage = self.builder.push_storage(self.source.clone(), MirStorageKind::Temporary, ty).map_err(capacity)?;
        let place = MirPlace::new(storage, [], ty);
        self.builder.push_operation(block, self.source.clone(), MirOperationKind::Store {
            kind: MirStoreKind::Initialize, destination: place.clone(), value,
        }, None).map_err(capacity)?;
        Ok(place)
    }

    pub(super) fn observed_place(&mut self, block: MirBlockId, value: MirOperand) -> Result<MirPlace, C::Error> {
        let mut place = self.place(block, value)?;
        loop {
            let data = self.context.semantic_values().type_data(place.ty());
            let TypeData::Borrow { target, .. } = data.as_ref() else { return Ok(place); };
            place = place.project(MirProjectionKind::Dereference, *target);
        }
    }

    pub(super) fn borrow_arguments(&mut self, block: MirBlockId, values: Vec<MirOperand>) -> Result<Vec<MirOperand>, C::Error> {
        values.into_iter().map(|value| {
            let place = self.place(block, value)?;
            let ty = self.context.semantic_values().intern_type(TypeData::Borrow { kind: BorrowKind::Shared, target: place.ty() })
                .map_err(crate::SyntheticLoweringError::SemanticValue)?;
            self.value(block, MirOperationKind::Borrow { kind: BorrowKind::Shared, place }, ty)
        }).collect()
    }

    pub(super) fn structural(&mut self, mut block: MirBlockId, operation: &CheckedTemplateOperation, ty: TypeId) -> Result<(MirBlockId, MirOperand), C::Error> {
        let value = match operation {
            CheckedTemplateOperation::Borrow { kind, operand } => {
                let (continued, value) = self.node(block, *operand)?;
                block = continued;
                let place = self.observed_place(block, value)?;
                self.value(block, MirOperationKind::Borrow { kind: *kind, place }, ty)?
            }
            CheckedTemplateOperation::Tuple(elements) | CheckedTemplateOperation::Array(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let (continued, value) = self.node(block, *element)?;
                    block = continued;
                    values.push(value);
                }
                let kind = if matches!(operation, CheckedTemplateOperation::Tuple(_)) { MirAggregateKind::Tuple } else { MirAggregateKind::Array };
                self.value(block, MirOperationKind::Aggregate(MirAggregate::new(kind, values)), ty)?
            }
            CheckedTemplateOperation::Product(fields) => {
                let TypeData::Named { definition: bray_symbols::NamedTypeSymbolId::Struct(definition), .. } = *self.context.semantic_values().type_data(ty) else {
                    panic!("checked product construction retains its struct type");
                };
                let mut inputs = Vec::with_capacity(fields.len());
                for field in fields.iter() {
                    let AnySymbolId::StructField(field_id) = self.context.predicate_symbol(field.field())? else { panic!("checked product field retains its declaration"); };
                    let (continued, value) = self.node(block, *field.value())?;
                    block = continued;
                    inputs.push(MirConstructionInput::new(ConstructionInputId::StructField(field_id), self.context.predicate_field_ordinal(field_id)?, value));
                }
                self.value(block, MirOperationKind::Construct(MirConstruction::new(ConstructionTarget::Struct(definition), inputs)), ty)?
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
            CheckedTemplateOperation::TupleElement { subject, index } => {
                let (continued, value) = self.node(block, *subject)?;
                block = continued;
                let place = self.observed_place(block, value)?;
                MirOperand::Copy(place.project(MirProjectionKind::TupleField(index.raw()), ty))
            }
            CheckedTemplateOperation::Index { subject, index, call: None } => {
                let (continued, subject) = self.node(block, *subject)?;
                let (continued, index) = self.node(continued, *index)?;
                block = continued;
                let place = self.observed_place(block, subject)?;
                MirOperand::Copy(place.project(MirProjectionKind::Index(index), ty))
            }
            CheckedTemplateOperation::Slice { subject, lower, upper, call: None } => {
                let (continued, subject) = self.node(block, *subject)?;
                block = continued;
                let mut bounds = Vec::new();
                for bound in [lower, upper] {
                    bounds.push(if let Some(bound) = bound {
                        let (continued, value) = self.node(block, *bound)?;
                        block = continued;
                        Some(value)
                    } else { None });
                }
                let place = self.observed_place(block, subject)?;
                MirOperand::Copy(place.project(MirProjectionKind::Slice { start: bounds.remove(0), end: bounds.remove(0) }, ty))
            }
            CheckedTemplateOperation::VariantTest { subject, variant } => {
                let (continued, subject) = self.node(block, *subject)?;
                block = continued;
                let AnySymbolId::UnionVariant(variant) = self.context.predicate_symbol(variant)? else { panic!("checked union test retains its variant"); };
                return self.variant_test(block, subject, variant);
            }
            CheckedTemplateOperation::Index { call: Some(_), .. } | CheckedTemplateOperation::Slice { call: Some(_), .. } => return self.custom_index(block, operation, ty),
            other => unreachable!("structural predicate dispatch received scalar operation {other:?}"),
        };
        Ok((block, value))
    }
}
