use super::encoding::OrderKey;
use super::encoding::{integer, real, sequence, term};
use crate::compilation::{CodegenPreparationError, Compilation, binder::CompilationBindingContext};
use bray_symbols::{
    ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind, GenericArgument,
    GenericSubstitutionId, ImplementationInstanceId, TraitApplicationId, TypeData, TypeId,
};

pub(super) struct OrderEncoder<'context, 'compilation> {
    pub(super) compilation: &'compilation Compilation,
    pub(super) context: &'context CompilationBindingContext<'compilation>,
}

impl OrderEncoder<'_, '_> {
    pub(super) fn substitution(
        &self,
        id: GenericSubstitutionId,
    ) -> Result<OrderKey, CodegenPreparationError> {
        let values = self.compilation.semantic_value_store()?;
        let substitution = values.generic_substitution_data(id);

        let arguments = substitution
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => self.ty(ty),
                GenericArgument::Constant(value) => self.constant(value),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(sequence(arguments))
    }

    pub(super) fn implementation(
        &self,
        id: ImplementationInstanceId,
    ) -> Result<OrderKey, CodegenPreparationError> {
        let values = self.compilation.semantic_value_store()?;
        let implementation = values.implementation_instance_data(id);

        Ok(sequence([
            self.symbol(implementation.definition().into_any())?,
            self.substitution(implementation.substitution())?,
        ]))
    }

    pub(super) fn trait_application(
        &self,
        id: TraitApplicationId,
    ) -> Result<OrderKey, CodegenPreparationError> {
        let values = self.compilation.semantic_value_store()?;
        let application = values.trait_application_data(id);

        Ok(sequence([
            self.symbol(application.definition().into())?,
            self.substitution(application.substitution())?,
        ]))
    }

    pub(super) fn ty(&self, id: TypeId) -> Result<OrderKey, CodegenPreparationError> {
        let values = self.compilation.semantic_value_store()?;
        let data = values.type_data(id);

        Ok(match data.as_ref() {
            TypeData::Named {
                definition,
                substitution,
            } => term(
                "named",
                [
                    self.symbol(definition.into_any())?,
                    self.substitution(*substitution)?,
                ],
            ),
            TypeData::Tuple(elements) => term(
                "tuple",
                elements
                    .iter()
                    .map(|ty| self.ty(*ty))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeData::Array { element, length } => {
                term("array", [self.ty(*element)?, self.constant(*length)?])
            }
            TypeData::FlexibleArray(element) => term("flexible_array", [self.ty(*element)?]),
            TypeData::Slice(element) => term("slice", [self.ty(*element)?]),
            TypeData::Generator(element) => term("generator", [self.ty(*element)?]),
            TypeData::Nullable(element) => term("nullable", [self.ty(*element)?]),
            TypeData::Borrow { kind, target } => term(
                "borrow",
                [kind.as_str().as_bytes().to_vec().into(), self.ty(*target)?],
            ),
            TypeData::TraitView(application) => {
                term("trait_view", [self.trait_application(*application)?])
            }
            TypeData::OwnedIndirection { storage, target } => {
                term("owned_indirection", [self.ty(*storage)?, self.ty(*target)?])
            }
            TypeData::Callable(callable) => self.callable(callable)?,
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. } => {
                panic!("closed static cleanup key contains unresolved type {data:?}")
            }
        })
    }

    pub(super) fn constant(&self, id: ConstantTermId) -> Result<OrderKey, CodegenPreparationError> {
        let values = self.compilation.semantic_value_store()?;
        let data = values.constant_term_data(id);

        match data.as_ref() {
            ConstantTermData::Value(value) => self.constant_value(*value),
            ConstantTermData::Typed { term: value, ty } => {
                Ok(term("typed", [self.ty(*ty)?, self.constant(*value)?]))
            }
            ConstantTermData::IntegerLiteral { ty, value } => Ok(term(
                match ty {
                    bray_symbols::TargetSizedIntegerType::Isize => "isize",
                    bray_symbols::TargetSizedIntegerType::Usize => "usize",
                },
                [integer(value)],
            )),
            ConstantTermData::CallableArgument(ordinal) => Ok(term(
                "callable_argument",
                [ordinal.raw().to_be_bytes().to_vec().into()],
            )),
            ConstantTermData::Parameter(parameter) => {
                Ok(term("parameter", [self.symbol((*parameter).into())?]))
            }
            ConstantTermData::TargetProperty(definition) => Ok(term(
                "target_property",
                [self.symbol((*definition).into())?],
            )),
            ConstantTermData::Unary { operation, operand } => {
                Ok(term(unary_name(*operation), [self.constant(*operand)?]))
            }
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => Ok(term(
                binary_name(*operation),
                [self.constant(*left)?, self.constant(*right)?],
            )),
            ConstantTermData::Conversion { operand, target } => Ok(term(
                "conversion",
                [self.constant(*operand)?, self.ty(*target)?],
            )),
            ConstantTermData::NullablePresent(value) => {
                Ok(term("nullable_present", [self.constant(*value)?]))
            }
            ConstantTermData::Tuple(elements) | ConstantTermData::Array(elements) => Ok(term(
                if matches!(data.as_ref(), ConstantTermData::Tuple(_)) {
                    "tuple"
                } else {
                    "array"
                },
                elements
                    .iter()
                    .map(|value| self.constant(*value))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            ConstantTermData::Product(fields) => Ok(term(
                "product",
                fields
                    .iter()
                    .map(|field| {
                        Ok(sequence([
                            self.symbol((*field.field()).into())?,
                            self.constant(*field.value())?,
                        ]))
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
            )),
            ConstantTermData::Union { variant, fields } => Ok(term(
                "union",
                [
                    self.symbol((*variant).into())?,
                    sequence(
                        fields
                            .iter()
                            .map(|field| {
                                Ok(sequence([
                                    self.symbol((*field.field()).into())?,
                                    self.constant(*field.value())?,
                                ]))
                            })
                            .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
                    ),
                ],
            )),
            ConstantTermData::DefinitionApplication {
                definition,
                substitution,
                selected_implementation,
            } => Ok(term(
                "definition",
                [
                    self.symbol(definition.into_any())?,
                    self.substitution(*substitution)?,
                    sequence(
                        selected_implementation
                            .map(|id| self.implementation(id))
                            .transpose()?,
                    ),
                ],
            )),
            ConstantTermData::Call {
                callable,
                selected_implementation,
                arguments,
            } => {
                let callable = values.callable_instance_data(*callable);

                Ok(term(
                    "call",
                    [
                        self.symbol(callable.definition().symbol())?,
                        self.substitution(callable.substitution())?,
                        sequence(
                            selected_implementation
                                .map(|id| self.implementation(id))
                                .transpose()?,
                        ),
                        sequence(
                            arguments
                                .iter()
                                .map(|argument| self.constant(*argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                    ],
                ))
            }
            ConstantTermData::PredicateCall {
                predicate,
                arguments,
            } => Ok(term(
                "predicate",
                [
                    self.symbol(predicate.definition().into_any())?,
                    self.substitution(predicate.substitution())?,
                    sequence(
                        arguments
                            .iter()
                            .map(|argument| self.constant(*argument))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                ],
            )),
            ConstantTermData::Projection(projection) => {
                use bray_symbols::ConstantProjectionKind;

                let selector = match projection.kind() {
                    ConstantProjectionKind::TupleElement(ordinal) => term(
                        "tuple_element",
                        [ordinal.raw().to_be_bytes().to_vec().into()],
                    ),
                    ConstantProjectionKind::ArrayElementOrdinal(index) => term(
                        "array_element_ordinal",
                        [index.raw().to_be_bytes().to_vec().into()],
                    ),
                    ConstantProjectionKind::ArrayElement(index) => {
                        term("array_element", [self.constant(index)?])
                    }
                    ConstantProjectionKind::ArraySlice { lower, upper } => term(
                        "array_slice",
                        [
                            sequence(lower.map(|value| self.constant(value)).transpose()?),
                            sequence(upper.map(|value| self.constant(value)).transpose()?),
                        ],
                    ),
                    ConstantProjectionKind::ProductField(field) => {
                        term("product_field", [self.symbol(field.into())?])
                    }
                    ConstantProjectionKind::UnionPayloadField(field) => {
                        term("union_payload_field", [self.symbol(field.into())?])
                    }
                    ConstantProjectionKind::NullableValue => term("nullable_value", []),
                };

                Ok(term(
                    "projection",
                    [self.constant(projection.subject())?, selector],
                ))
            }
        }
    }

    fn constant_value(&self, id: ConstantValueId) -> Result<OrderKey, CodegenPreparationError> {
        let values = self.compilation.semantic_value_store()?;
        let data = values.constant_value_data(id);

        let value = match data.kind() {
            ConstantValueKind::Boolean(value) => term("boolean", [vec![u8::from(*value)].into()]),
            ConstantValueKind::Character(value) => term(
                "character",
                [u32::from(*value).to_be_bytes().to_vec().into()],
            ),
            ConstantValueKind::Integer(value) => term("integer", [integer(value)]),
            ConstantValueKind::Real(value) => real(*value),
            ConstantValueKind::Complex {
                real: re,
                imaginary,
            } => term("complex", [real(*re), real(*imaginary)]),
            ConstantValueKind::String(value) => term("string", [value.as_bytes().to_vec().into()]),
            ConstantValueKind::Unit => term("unit", []),
            ConstantValueKind::NullableAbsent => term("nullable_absent", []),
            ConstantValueKind::NullablePresent(value) => {
                term("nullable_present", [self.constant_value(*value)?])
            }
            ConstantValueKind::Tuple(elements) | ConstantValueKind::Array(elements) => term(
                if matches!(data.kind(), ConstantValueKind::Tuple(_)) {
                    "tuple"
                } else {
                    "array"
                },
                elements
                    .iter()
                    .map(|value| self.constant_value(*value))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantValueKind::Product(fields) => term(
                "product",
                fields
                    .iter()
                    .map(|field| {
                        Ok(sequence([
                            self.symbol((*field.field()).into())?,
                            self.constant_value(*field.value())?,
                        ]))
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
            ),
            ConstantValueKind::Union { variant, fields } => term(
                "union",
                [
                    self.symbol((*variant).into())?,
                    sequence(
                        fields
                            .iter()
                            .map(|field| {
                                Ok(sequence([
                                    self.symbol((*field.field()).into())?,
                                    self.constant_value(*field.value())?,
                                ]))
                            })
                            .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
                    ),
                ],
            ),
            ConstantValueKind::Error | ConstantValueKind::StaticAddress(_) => {
                panic!("invalid closed generic argument in static cleanup key: {data:?}")
            }
        };

        Ok(term("constant", [self.ty(data.ty())?, value]))
    }
}

fn unary_name(operation: bray_symbols::ConstantUnaryOperation) -> &'static str {
    use bray_symbols::ConstantUnaryOperation;

    match operation {
        ConstantUnaryOperation::Identity => "identity",
        ConstantUnaryOperation::Negate => "negate",
        ConstantUnaryOperation::LogicalNot => "logical_not",
        ConstantUnaryOperation::BitwiseNot => "bitwise_not",
        ConstantUnaryOperation::PredicateTrust => "predicate_trust",
        ConstantUnaryOperation::BorrowObservation => "borrow_observation",
        ConstantUnaryOperation::EntryCondition => "entry_condition",
    }
}

fn binary_name(operation: bray_symbols::ConstantBinaryOperation) -> &'static str {
    use bray_symbols::ConstantBinaryOperation;

    match operation {
        ConstantBinaryOperation::Add => "add",
        ConstantBinaryOperation::Subtract => "subtract",
        ConstantBinaryOperation::Multiply => "multiply",
        ConstantBinaryOperation::Divide => "divide",
        ConstantBinaryOperation::Remainder => "remainder",
        ConstantBinaryOperation::Exponentiate => "exponentiate",
        ConstantBinaryOperation::LogicalAnd => "logical_and",
        ConstantBinaryOperation::LogicalOr => "logical_or",
        ConstantBinaryOperation::BitwiseAnd => "bitwise_and",
        ConstantBinaryOperation::BitwiseOr => "bitwise_or",
        ConstantBinaryOperation::BitwiseXor => "bitwise_xor",
        ConstantBinaryOperation::ShiftLeft => "shift_left",
        ConstantBinaryOperation::ShiftRight => "shift_right",
        ConstantBinaryOperation::Equal => "equal",
        ConstantBinaryOperation::NotEqual => "not_equal",
        ConstantBinaryOperation::Less => "less",
        ConstantBinaryOperation::LessOrEqual => "less_or_equal",
        ConstantBinaryOperation::Greater => "greater",
        ConstantBinaryOperation::GreaterOrEqual => "greater_or_equal",
    }
}

#[cfg(test)]
mod tests {
    use super::OrderEncoder;
    use crate::fact::CancellationToken;
    use bray_symbols::{
        ConstantBinaryOperation, ConstantProjection, ConstantProjectionKind, ConstantTermData,
        SymbolOrdinal,
    };

    fn symbolic_key(padding: bool, ordinal: u32, upper: bool) -> std::sync::Arc<[u8]> {
        let compilation = crate::test_support::compilation("module app;");
        let cancellation = CancellationToken::new();
        let context = compilation.binding_context(&cancellation).unwrap();
        let values = compilation.semantic_value_store().unwrap();

        if padding {
            values
                .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(99)))
                .unwrap();
        }

        let argument = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(
                ordinal,
            )))
            .unwrap();

        let sum = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::Add,
                left: argument,
                right: argument,
            })
            .unwrap();

        let projection = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                argument,
                ConstantProjectionKind::ArraySlice {
                    lower: Some(sum),
                    upper: upper.then_some(sum),
                },
            )))
            .unwrap();

        OrderEncoder {
            compilation: &compilation,
            context: &context,
        }
        .constant(projection)
        .unwrap()
        .into_bytes()
    }

    #[test]
    fn callable_dependency_terms_compare_structure_independently_of_interning_order() {
        let first = symbolic_key(false, 0, false);

        assert_eq!(first, symbolic_key(true, 0, false));
        assert!(first < symbolic_key(false, 1, false));
        assert_ne!(first, symbolic_key(false, 0, true));
    }
}
