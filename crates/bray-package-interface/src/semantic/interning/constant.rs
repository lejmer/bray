use std::sync::Arc;

use bray_symbols::{
    ConstantField, ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantValueData,
    ConstantValueKind, SemanticValueStore, StructFieldSymbolId, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

use crate::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantValueKind,
    InterfaceSemantics,
};

use super::common::{
    collect_ids, invalid_symbol, resolve_constant_definition, resolve_exact, resolve_symbol,
};
use super::{InterfaceSemanticInternError, InterfaceSymbolResolver, InternState};

impl InternState {
    pub(super) fn intern_constant_values(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.constant_values.iter().enumerate() {
            if self.constant_values[index].is_some() {
                continue;
            }

            let Some(ty) = self.type_id(input.ty) else {
                continue;
            };

            let Some(kind) = self.convert_constant_value(&input.kind, symbols)? else {
                continue;
            };

            self.constant_values[index] =
                Some(store.intern_constant_value(ConstantValueData::new(ty, kind))?);
        }

        Ok(())
    }

    pub(super) fn convert_constant_value(
        &self,
        input: &InterfaceConstantValueKind,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<ConstantValueKind>, InterfaceSemanticInternError> {
        Ok(match input {
            InterfaceConstantValueKind::Boolean(value) => Some(ConstantValueKind::Boolean(*value)),
            InterfaceConstantValueKind::Character(value) => {
                Some(ConstantValueKind::Character(*value))
            }
            InterfaceConstantValueKind::Integer(value) => {
                // Integer payloads are Arc-backed canonical values and sharing avoids a deep copy.
                Some(ConstantValueKind::Integer(value.clone()))
            }
            InterfaceConstantValueKind::Real(value) => Some(ConstantValueKind::Real(*value)),
            InterfaceConstantValueKind::Complex { real, imaginary } => {
                Some(ConstantValueKind::Complex {
                    real: *real,
                    imaginary: *imaginary,
                })
            }
            InterfaceConstantValueKind::String(value) => {
                Some(ConstantValueKind::String(Arc::clone(value)))
            }
            InterfaceConstantValueKind::Unit => Some(ConstantValueKind::Unit),
            InterfaceConstantValueKind::NullableAbsent => Some(ConstantValueKind::NullableAbsent),
            InterfaceConstantValueKind::NullablePresent(id) => self
                .constant_value_id(*id)
                .map(ConstantValueKind::NullablePresent),
            InterfaceConstantValueKind::Tuple(values) => {
                collect_ids(values, |id| self.constant_value_id(*id)).map(ConstantValueKind::tuple)
            }
            InterfaceConstantValueKind::Array(values) => {
                collect_ids(values, |id| self.constant_value_id(*id)).map(ConstantValueKind::array)
            }
            InterfaceConstantValueKind::Product(fields) => {
                let mut values = Vec::with_capacity(fields.len());

                for field in fields.iter() {
                    let Some(value) = self.constant_value_id(*field.value()) else {
                        return Ok(None);
                    };

                    values.push(ConstantField::new(
                        resolve_exact::<StructFieldSymbolId>(symbols, field.field())?,
                        value,
                    ));
                }

                Some(ConstantValueKind::product(values))
            }
            InterfaceConstantValueKind::Union { variant, fields } => {
                let mut values = Vec::with_capacity(fields.len());

                for field in fields.iter() {
                    let Some(value) = self.constant_value_id(*field.value()) else {
                        return Ok(None);
                    };

                    values.push(ConstantField::new(
                        resolve_exact::<UnionPayloadFieldSymbolId>(symbols, field.field())?,
                        value,
                    ));
                }

                Some(ConstantValueKind::union(
                    resolve_exact::<UnionVariantSymbolId>(symbols, variant)?,
                    values,
                ))
            }
        })
    }

    pub(super) fn intern_constant_terms(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.constant_terms.iter().enumerate() {
            if self.constant_terms[index].is_some() {
                continue;
            }

            let Some(data) = self.convert_constant_term(input, symbols)? else {
                continue;
            };

            self.constant_terms[index] = Some(store.intern_constant_term(data)?);
        }

        Ok(())
    }

    pub(super) fn convert_constant_term(
        &self,
        input: &InterfaceConstantTerm,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<ConstantTermData>, InterfaceSemanticInternError> {
        Ok(match input {
            InterfaceConstantTerm::Value(id) => {
                self.constant_value_id(*id).map(ConstantTermData::Value)
            }
            InterfaceConstantTerm::IntegerLiteral { ty, value } => {
                Some(ConstantTermData::IntegerLiteral {
                    ty: *ty,
                    // The interface snapshot remains immutable while the semantic store owns its key.
                    value: value.clone(),
                })
            }
            InterfaceConstantTerm::Parameter(parameter) => Some(ConstantTermData::Parameter(
                resolve_exact(symbols, parameter)?,
            )),
            InterfaceConstantTerm::TargetProperty(record) => Some(
                ConstantTermData::TargetProperty(resolve_exact(symbols, record)?),
            ),
            InterfaceConstantTerm::Unary { operation, operand } => self
                .constant_term_id(*operand)
                .map(|operand| ConstantTermData::Unary {
                    operation: *operation,
                    operand,
                }),
            InterfaceConstantTerm::Binary {
                operation,
                left,
                right,
            } => {
                let (Some(left), Some(right)) =
                    (self.constant_term_id(*left), self.constant_term_id(*right))
                else {
                    return Ok(None);
                };

                Some(ConstantTermData::Binary {
                    operation: *operation,
                    left,
                    right,
                })
            }
            InterfaceConstantTerm::Conversion { operand, target } => {
                let (Some(operand), Some(target)) =
                    (self.constant_term_id(*operand), self.type_id(*target))
                else {
                    return Ok(None);
                };

                Some(ConstantTermData::Conversion { operand, target })
            }
            InterfaceConstantTerm::NullablePresent(value) => self
                .constant_term_id(*value)
                .map(ConstantTermData::NullablePresent),
            InterfaceConstantTerm::Tuple(values) => {
                collect_ids(values, |id| self.constant_term_id(*id)).map(ConstantTermData::tuple)
            }
            InterfaceConstantTerm::Array(values) => {
                collect_ids(values, |id| self.constant_term_id(*id)).map(ConstantTermData::array)
            }
            InterfaceConstantTerm::Product(fields) => {
                let mut values = Vec::with_capacity(fields.len());

                for field in fields.iter() {
                    let Some(value) = self.constant_term_id(*field.value()) else {
                        return Ok(None);
                    };

                    values.push(ConstantField::new(
                        resolve_exact::<StructFieldSymbolId>(symbols, field.field())?,
                        value,
                    ));
                }

                Some(ConstantTermData::product(values))
            }
            InterfaceConstantTerm::Union { variant, fields } => {
                let mut values = Vec::with_capacity(fields.len());

                for field in fields.iter() {
                    let Some(value) = self.constant_term_id(*field.value()) else {
                        return Ok(None);
                    };

                    values.push(ConstantField::new(
                        resolve_exact::<UnionPayloadFieldSymbolId>(symbols, field.field())?,
                        value,
                    ));
                }

                Some(ConstantTermData::union(
                    resolve_exact::<UnionVariantSymbolId>(symbols, variant)?,
                    values,
                ))
            }
            InterfaceConstantTerm::DefinitionApplication {
                definition,
                substitution,
                selected_implementation,
            } => {
                let Some(substitution) = self.substitution_id(*substitution) else {
                    return Ok(None);
                };

                let selected_implementation = match selected_implementation {
                    Some(id) => {
                        let Some(id) = self.implementation_instance_id(*id) else {
                            return Ok(None);
                        };

                        Some(id)
                    }
                    None => None,
                };

                Some(ConstantTermData::DefinitionApplication {
                    definition: resolve_constant_definition(symbols, definition)?,
                    substitution,
                    selected_implementation,
                })
            }
            InterfaceConstantTerm::Call {
                callable,
                selected_implementation,
                arguments,
            } => {
                let Some(callable) = self.callable_instance_id(*callable) else {
                    return Ok(None);
                };

                let selected_implementation = match selected_implementation {
                    Some(id) => {
                        let Some(id) = self.implementation_instance_id(*id) else {
                            return Ok(None);
                        };

                        Some(id)
                    }
                    None => None,
                };

                let Some(arguments) = collect_ids(arguments, |id| self.constant_term_id(*id))
                else {
                    return Ok(None);
                };

                Some(ConstantTermData::call(
                    callable,
                    selected_implementation,
                    arguments,
                ))
            }
            InterfaceConstantTerm::PredicateCall {
                predicate,
                substitution,
                arguments,
            } => {
                let predicate_symbol = resolve_symbol(symbols, predicate)?;

                let predicate =
                    bray_symbols::PredicateDefinitionSymbolId::try_from_any(predicate_symbol)
                        .ok_or_else(|| invalid_symbol(predicate))?;

                let Some(substitution) = self.substitution_id(*substitution) else {
                    return Ok(None);
                };

                let Some(arguments) = collect_ids(arguments, |id| self.constant_term_id(*id))
                else {
                    return Ok(None);
                };

                Some(ConstantTermData::predicate_call(
                    bray_symbols::PredicateInstanceData::new(predicate, substitution),
                    arguments,
                ))
            }
            InterfaceConstantTerm::Projection { subject, kind } => {
                let Some(subject) = self.constant_term_id(*subject) else {
                    return Ok(None);
                };

                let Some(kind) = self.convert_constant_projection(kind, symbols)? else {
                    return Ok(None);
                };

                Some(ConstantTermData::Projection(ConstantProjection::new(
                    subject, kind,
                )))
            }
        })
    }

    pub(super) fn convert_constant_projection(
        &self,
        input: &InterfaceConstantProjection,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<ConstantProjectionKind>, InterfaceSemanticInternError> {
        Ok(match input {
            InterfaceConstantProjection::TupleElement(ordinal) => {
                Some(ConstantProjectionKind::TupleElement(*ordinal))
            }
            InterfaceConstantProjection::ArrayElement(id) => self
                .constant_term_id(*id)
                .map(ConstantProjectionKind::ArrayElement),
            InterfaceConstantProjection::ProductField(field) => {
                Some(ConstantProjectionKind::ProductField(resolve_exact::<
                    StructFieldSymbolId,
                >(
                    symbols, field
                )?))
            }
            InterfaceConstantProjection::UnionPayloadField(field) => {
                Some(ConstantProjectionKind::UnionPayloadField(resolve_exact::<
                    UnionPayloadFieldSymbolId,
                >(
                    symbols, field
                )?))
            }
            InterfaceConstantProjection::NullableValue => {
                Some(ConstantProjectionKind::NullableValue)
            }
        })
    }
}
