use bray_bound_tree::{CheckedMemoryOperationKind, ConstructionInputId, ConstructionTarget};
use bray_compiler_known::{CompilerKnownDeclarationKey, ImplementationHook, RepresentationRole};
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirBlockKind, MirConstruction, MirConstructionInput, MirEdge,
    MirFieldReference, MirImmediateValue, MirMemoryOperation, MirOperand, MirOperationKind,
    MirPlace, MirProjectionKind, MirStoreKind, MirTerminatorKind,
};
use bray_symbols::{BorrowKind, NamedTypeSymbolId, StructFieldSymbolId, TypeData, TypeId};

use crate::lowering::LoweringError;

pub(crate) struct GeneratorIterationLowerer<'a> {
    pub(crate) values: &'a bray_symbols::SemanticValueStore,
    pub(crate) symbols: &'a bray_symbols::AvailableCompilerKnownSymbols,
    pub(crate) builder: &'a mut bray_ir::MirUnitBuilder,
    pub(crate) source: bray_ir::MirSourceAnchor,
}

impl GeneratorIterationLowerer<'_> {
    pub(crate) fn generator_cursor(
        &mut self,
        mut block: MirBlockId,
        value: MirOperand,
        generator_type: TypeId,
        cursor_type: TypeId,
        hook: ImplementationHook,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let source = self.source.clone();

        let TypeData::Generator(element) = *self.values.type_data(generator_type) else {
            panic!("generator iteration requires its selected generator type");
        };

        let inputs = if hook == ImplementationHook::GeneratorSharedIterate {
            let integer = self.representation_type(RepresentationRole::ScalarUsize)?;
            let zero = crate::operand::integer_constant(self.values, integer, 0)?;

            vec![
                MirConstructionInput::new(
                    ConstructionInputId::StructField(
                        self.generator_field("GeneratorSharedCursorValues"),
                    ),
                    0,
                    value,
                ),
                MirConstructionInput::new(
                    ConstructionInputId::StructField(
                        self.generator_field("GeneratorSharedCursorIndex"),
                    ),
                    1,
                    zero,
                ),
            ]
        } else {
            assert_eq!(hook, ImplementationHook::GeneratorMoveIterate);

            let owned = self.materialize_synthetic_place(
                block,
                Self::retained_source(&source),
                value,
                generator_type,
            )?;

            block = self.reverse_generator(block, &owned, element)?;

            vec![MirConstructionInput::new(
                ConstructionInputId::StructField(self.generator_field("GeneratorMoveCursorValues")),
                0,
                MirOperand::Move(owned),
            )]
        };

        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(definition),
            ..
        } = *self.values.type_data(cursor_type)
        else {
            panic!("selected generator cursor is a compiler-owned struct");
        };

        let result = self.push_typed_value_operation(
            block,
            source,
            MirOperationKind::Construct(MirConstruction::new(
                ConstructionTarget::Struct(definition),
                inputs,
            )),
            cursor_type,
        )?;

        Ok((block, result))
    }

    pub(crate) fn generator_next(
        &mut self,
        block: MirBlockId,
        cursor: MirPlace,
        result: TypeId,
        hook: ImplementationHook,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let source = self.source.clone();

        let TypeData::Nullable(element) = *self.values.type_data(result) else {
            panic!("selected generator next returns a nullable element");
        };

        if hook == ImplementationHook::GeneratorMoveNext {
            let generator = self.values.intern_type(TypeData::Generator(element))?;

            let values = cursor.project(
                MirProjectionKind::Field(MirFieldReference::Struct(
                    self.generator_field("GeneratorMoveCursorValues"),
                )),
                generator,
            );

            let borrowed = self.generator_buffer_borrow(block, &source, values)?;
            let integer = self.representation_type(RepresentationRole::ScalarUsize)?;
            let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

            let count = self.generator_memory_value(
                block,
                &source,
                MirMemoryOperation::new(
                    CheckedMemoryOperationKind::RawBufferInitializedCount,
                    [MirOperand::Copy(Self::retained_place(&borrowed))],
                    [borrowed.ty()],
                    Some(integer),
                ),
            )?;

            let zero = crate::operand::integer_constant(self.values, integer, 0)?;

            let available = self.push_typed_value_operation(
                block,
                Self::retained_source(&source),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::GreaterThan,
                    left: count,
                    right: zero,
                },
                boolean,
            )?;

            let item = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let join = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let output =
                self.builder
                    .push_block_parameter(join, Self::retained_source(&source), result)?;

            self.builder.set_terminator(
                block,
                Self::retained_source(&source),
                MirTerminatorKind::Branch {
                    condition: available,
                    then_edge: MirEdge::new(item, []),
                    else_edge: MirEdge::new(
                        join,
                        [Self::immediate_operand(
                            result,
                            MirImmediateValue::NullableAbsent,
                        )],
                    ),
                },
            );

            let value = self.generator_memory_value(
                item,
                &source,
                MirMemoryOperation::new(
                    CheckedMemoryOperationKind::RawBufferPop { element },
                    [MirOperand::Copy(Self::retained_place(&borrowed))],
                    [borrowed.ty()],
                    Some(element),
                ),
            )?;

            let present =
                self.push_nullable_present(item, Self::retained_source(&source), value, result)?;

            self.builder.set_terminator(
                item,
                Self::retained_source(&source),
                MirTerminatorKind::Goto(MirEdge::new(join, [present])),
            );

            return Ok((join, MirOperand::Value(output)));
        }

        assert_eq!(hook, ImplementationHook::GeneratorSharedNext);

        let integer = self.representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let slice = self.values.intern_type(TypeData::Slice(element))?;

        let borrowed = self.values.intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: slice,
        })?;

        let generator = self.values.intern_type(TypeData::Generator(element))?;

        let generator_borrow = self.values.intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: generator,
        })?;

        let owner = cursor.project(
            MirProjectionKind::Field(MirFieldReference::Struct(
                self.generator_field("GeneratorSharedCursorValues"),
            )),
            generator_borrow,
        );

        let values = self.generator_memory_value(
            block,
            &source,
            MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferInitializedSlice,
                [MirOperand::Copy(owner)],
                [generator_borrow],
                Some(borrowed),
            ),
        )?;

        let values = self.materialize_synthetic_place(
            block,
            Self::retained_source(&source),
            values,
            borrowed,
        )?;

        let index = cursor.project(
            MirProjectionKind::Field(MirFieldReference::Struct(
                self.generator_field("GeneratorSharedCursorIndex"),
            )),
            integer,
        );

        let length = self.generator_memory_value(
            block,
            &source,
            MirMemoryOperation::new(
                CheckedMemoryOperationKind::SequenceLength,
                [MirOperand::Copy(Self::retained_place(&values))],
                [borrowed],
                Some(integer),
            ),
        )?;

        let available = self.push_typed_value_operation(
            block,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::LessThan,
                left: MirOperand::Copy(Self::retained_place(&index)),
                right: length,
            },
            boolean,
        )?;

        let item = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let output =
            self.builder
                .push_block_parameter(join, Self::retained_source(&source), result)?;

        let absent = Self::immediate_operand(result, MirImmediateValue::NullableAbsent);

        self.builder.set_terminator(
            block,
            Self::retained_source(&source),
            MirTerminatorKind::Branch {
                condition: available,
                then_edge: MirEdge::new(item, []),
                else_edge: MirEdge::new(join, [absent]),
            },
        );

        let element_place = values
            .project(MirProjectionKind::Dereference, slice)
            .project(
                MirProjectionKind::Index(MirOperand::Copy(Self::retained_place(&index))),
                element,
            );

        let present = self.push_nullable_present(
            item,
            Self::retained_source(&source),
            MirOperand::Copy(element_place),
            result,
        )?;

        let one = crate::operand::integer_constant(self.values, integer, 1)?;

        let next = self.push_typed_value_operation(
            item,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Add,
                left: MirOperand::Copy(Self::retained_place(&index)),
                right: one,
            },
            integer,
        )?;

        self.builder.push_operation(
            item,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: index,
                value: next,
            },
            None,
        )?;

        self.builder.set_terminator(
            item,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(join, [present])),
        );

        Ok((join, MirOperand::Value(output)))
    }

    fn reverse_generator(
        &mut self,
        block: MirBlockId,
        owner: &MirPlace,
        element: TypeId,
    ) -> Result<MirBlockId, LoweringError> {
        let source = self.source.clone();
        let integer = self.representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let buffer = self.generator_buffer_borrow(block, &source, Self::retained_place(owner))?;

        let count = self.generator_memory_value(
            block,
            &source,
            MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferInitializedCount,
                [MirOperand::Copy(Self::retained_place(&buffer))],
                [buffer.ty()],
                Some(integer),
            ),
        )?;

        let slice = self.values.intern_type(TypeData::Slice(element))?;

        let borrowed = self.values.intern_type(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: slice,
        })?;

        let values = self.generator_memory_value(
            block,
            &source,
            MirMemoryOperation::new(
                CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
                [MirOperand::Copy(buffer.clone())],
                [buffer.ty()],
                Some(borrowed),
            ),
        )?;

        let values = self.materialize_synthetic_place(
            block,
            Self::retained_source(&source),
            values,
            borrowed,
        )?;

        let zero = crate::operand::integer_constant(self.values, integer, 0)?;
        let one = crate::operand::integer_constant(self.values, integer, 1)?;
        let two = crate::operand::integer_constant(self.values, integer, 2)?;

        let half = self.push_typed_value_operation(
            block,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Divide,
                left: Self::retained_operand(&count),
                right: two,
            },
            integer,
        )?;

        let count = self.materialize_synthetic_place(
            block,
            Self::retained_source(&source),
            count,
            integer,
        )?;

        let half =
            self.materialize_synthetic_place(block, Self::retained_source(&source), half, integer)?;

        let header = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let index =
            self.builder
                .push_block_parameter(header, Self::retained_source(&source), integer)?;

        let index = self.materialize_synthetic_place(
            header,
            Self::retained_source(&source),
            MirOperand::Value(index),
            integer,
        )?;

        let index = MirOperand::Copy(index);

        let body = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let done = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
            block,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(header, [zero])),
        );

        let condition = self.push_typed_value_operation(
            header,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::LessThan,
                left: Self::retained_operand(&index),
                right: MirOperand::Copy(half),
            },
            boolean,
        )?;

        self.builder.set_terminator(
            header,
            Self::retained_source(&source),
            MirTerminatorKind::Branch {
                condition,
                then_edge: MirEdge::new(body, []),
                else_edge: MirEdge::new(done, []),
            },
        );

        let right = self.push_typed_value_operation(
            body,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Subtract,
                left: MirOperand::Copy(count),
                right: Self::retained_operand(&index),
            },
            integer,
        )?;

        let right = self.push_typed_value_operation(
            body,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Subtract,
                left: right,
                right: Self::retained_operand(&one),
            },
            integer,
        )?;

        let slice_place = values.project(MirProjectionKind::Dereference, slice);

        let left = slice_place.project(
            MirProjectionKind::Index(Self::retained_operand(&index)),
            element,
        );

        let right = slice_place.project(MirProjectionKind::Index(right), element);

        // Both slots stay within the admitted initialized prefix. No fallible work occurs during the swap.
        let temporary = self.materialize_synthetic_place(
            body,
            Self::retained_source(&source),
            MirOperand::Move(Self::retained_place(&left)),
            element,
        )?;

        self.builder.push_operation(
            body,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: left,
                value: MirOperand::Move(Self::retained_place(&right)),
            },
            None,
        )?;

        self.builder.push_operation(
            body,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: right,
                value: MirOperand::Move(temporary),
            },
            None,
        )?;

        let next = self.push_typed_value_operation(
            body,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Add,
                left: index,
                right: one,
            },
            integer,
        )?;

        self.builder.set_terminator(
            body,
            source,
            MirTerminatorKind::Goto(MirEdge::new(header, [next])),
        );

        Ok(done)
    }

    fn generator_field(&self, name: &str) -> StructFieldSymbolId {
        let key =
            CompilerKnownDeclarationKey::try_new(name).expect("generator field uses a catalog key");

        self.symbols
            .declaration_symbol(&key)
            .expect("selected generator cursor retains its catalog fields")
    }

    fn push_typed_value_operation(
        &mut self,
        block: MirBlockId,
        source: bray_ir::MirSourceAnchor,
        operation: MirOperationKind,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self
            .builder
            .push_operation(block, source, operation, Some(ty))?;

        Ok(MirOperand::Value(
            commit
                .result()
                .expect("typed generator operation produces a value"),
        ))
    }

    fn materialize_synthetic_place(
        &mut self,
        block: MirBlockId,
        source: bray_ir::MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
    ) -> Result<MirPlace, LoweringError> {
        let storage =
            self.builder
                .push_storage(source.clone(), bray_ir::MirStorageKind::Temporary, ty)?;

        let place = MirPlace::new(storage, [], ty);

        self.builder.push_operation(
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value,
            },
            None,
        )?;

        Ok(place)
    }

    fn push_nullable_present(
        &mut self,
        block: MirBlockId,
        source: bray_ir::MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        self.push_typed_value_operation(
            block,
            source,
            MirOperationKind::Aggregate(bray_ir::MirAggregate::new(
                bray_ir::MirAggregateKind::NullablePresent,
                [value],
            )),
            ty,
        )
    }

    fn generator_buffer_borrow(
        &mut self,
        block: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        place: MirPlace,
    ) -> Result<MirPlace, LoweringError> {
        let kind = BorrowKind::Mutable;

        let ty = self.values.intern_type(TypeData::Borrow {
            kind,
            target: place.ty(),
        })?;

        let value = self.push_typed_value_operation(
            block,
            source.clone(),
            MirOperationKind::Borrow { kind, place },
            ty,
        )?;

        self.materialize_synthetic_place(block, source.clone(), value, ty)
    }

    fn generator_memory_value(
        &mut self,
        block: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        memory: MirMemoryOperation,
    ) -> Result<MirOperand, LoweringError> {
        let ty = memory
            .result_type()
            .expect("generator memory observation produces a value");

        self.push_typed_value_operation(
            block,
            source.clone(),
            MirOperationKind::Memory(Box::new(memory)),
            ty,
        )
    }

    fn representation_type(&self, role: RepresentationRole) -> Result<TypeId, LoweringError> {
        let symbol = self
            .symbols
            .representation_symbol::<bray_symbols::StructSymbolId>(role)
            .expect("generator protocol requires its scalar representation");

        Ok(self
            .values
            .intern_non_generic_named_type(NamedTypeSymbolId::Struct(symbol))?)
    }

    fn retained_source(source: &bray_ir::MirSourceAnchor) -> bray_ir::MirSourceAnchor {
        source.clone()
    }
    fn retained_place(place: &MirPlace) -> MirPlace {
        place.clone()
    }
    fn retained_operand(operand: &MirOperand) -> MirOperand {
        operand.clone()
    }
    fn immediate_operand(ty: TypeId, value: MirImmediateValue) -> MirOperand {
        MirOperand::Immediate { value, ty }
    }
}

/// Realizes one selected generator protocol operation as an ordinary callable body.
/// This is also the implementation used when the operation is expanded directly in source MIR.
#[expect(
    clippy::too_many_arguments,
    reason = "callable synthesis retains its selected operation, closed semantic signature, stores, identity, and target"
)]
pub fn lower_generator_callable(
    values: &bray_symbols::SemanticValueStore,
    symbols: &bray_symbols::AvailableCompilerKnownSymbols,
    unit: bray_ir::MirUnitId,
    callable: bray_symbols::CallableInstanceData,
    hook: ImplementationHook,
    parameters: &[TypeId],
    result: TypeId,
    target: &bray_ir::MirTargetContract,
) -> Result<bray_ir::MirUnit, crate::SyntheticLoweringError> {
    let lower = || -> Result<bray_ir::MirUnit, LoweringError> {
        let [parameter] = parameters else {
            panic!("generator protocol callable retains its sole receiver");
        };

        let source = bray_ir::MirSourceAnchor::CompilerProvidedCallable(callable.definition());

        let mut builder = bray_ir::MirUnitBuilder::for_compiler_provided_callable(
            unit,
            callable.definition(),
            target.clone(),
        );

        let entry = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

        let storage = builder.push_storage(
            source.clone(),
            bray_ir::MirStorageKind::Parameter(0),
            *parameter,
        )?;

        let input = MirPlace::new(storage, [], *parameter);

        let mut lowering = GeneratorIterationLowerer {
            values,
            symbols,
            builder: &mut builder,
            source: source.clone(),
        };

        let (completed, value) = match hook {
            ImplementationHook::GeneratorSharedIterate
            | ImplementationHook::GeneratorMoveIterate => {
                let generator = match values.type_data(*parameter).as_ref() {
                    TypeData::Borrow { target, .. } => *target,
                    TypeData::Generator(_) => *parameter,
                    _ => panic!("selected generator iterate retains its generator input"),
                };

                let value = if hook == ImplementationHook::GeneratorMoveIterate {
                    MirOperand::Move(input)
                } else {
                    MirOperand::Copy(input)
                };

                lowering.generator_cursor(entry, value, generator, result, hook)?
            }
            ImplementationHook::GeneratorSharedNext | ImplementationHook::GeneratorMoveNext => {
                let TypeData::Borrow {
                    kind: BorrowKind::Mutable,
                    target,
                } = *values.type_data(*parameter)
                else {
                    panic!("selected generator next retains its mutable cursor input");
                };

                lowering.generator_next(
                    entry,
                    input.project(MirProjectionKind::Dereference, target),
                    result,
                    hook,
                )?
            }
            _ => panic!("generator callable synthesis requires its selected generator operation"),
        };

        builder.set_terminator(
            completed,
            source,
            MirTerminatorKind::Return {
                value: Some(value),
                report: None,
            },
        );

        Ok(builder.finish(entry))
    };

    lower().map_err(|cause| match cause {
        LoweringError::SemanticValue(cause) => crate::SyntheticLoweringError::SemanticValue(cause),
        LoweringError::MirCapacity(cause) => crate::SyntheticLoweringError::Capacity(cause),
    })
}
