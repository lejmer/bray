use super::{
    CallableInstanceData, CallableParameterData, CallableTypeData, ConstantProjection,
    ConstantProjectionKind, ConstantTermData, ConstantTermId, GenericArgument,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData, SemanticValueStore,
    SemanticValueStoreError, TraitApplicationData, TypeData, TypeId,
};
use crate::GenericParameterSymbolId;

impl SemanticValueStore {
    /// Applies one generic substitution throughout a canonical semantic type.
    pub fn substitute_type(
        &self,
        ty: TypeId,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;

        self.substitute_type_data(ty, &substitution)
    }

    fn substitute_type_data(
        &self,
        ty: TypeId,
        substitution: &GenericSubstitutionData,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let data = self.type_data(ty)?;

        if let TypeData::TypeParameter(parameter) = data.as_ref()
            && let Some(GenericArgument::Type(argument)) =
                substitution.argument_for(GenericParameterSymbolId::Type(*parameter))
        {
            return Ok(argument);
        }

        let substituted = match data.as_ref() {
            TypeData::Error | TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => {
                return Ok(ty);
            }
            TypeData::Named {
                definition,
                substitution: nested,
            } => TypeData::Named {
                definition: *definition,
                substitution: self.substitute_generic_substitution(*nested, substitution)?,
            },
            TypeData::AssociatedTypeProjection {
                subject,
                application,
                member,
            } => {
                let application = self.trait_application_data(*application)?;
                let nested =
                    self.substitute_generic_substitution(application.substitution(), substitution)?;
                let application = self.intern_trait_application(TraitApplicationData::new(
                    application.definition(),
                    nested,
                ))?;

                TypeData::AssociatedTypeProjection {
                    subject: self.substitute_type_data(*subject, substitution)?,
                    application,
                    member: *member,
                }
            }
            TypeData::Tuple(elements) => TypeData::tuple(
                elements
                    .iter()
                    .map(|element| self.substitute_type_data(*element, substitution))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeData::Array { element, length } => TypeData::Array {
                element: self.substitute_type_data(*element, substitution)?,
                length: self.substitute_constant_term(*length, substitution)?,
            },
            TypeData::Slice(element) => {
                TypeData::Slice(self.substitute_type_data(*element, substitution)?)
            }
            TypeData::Nullable(target) => {
                TypeData::Nullable(self.substitute_type_data(*target, substitution)?)
            }
            TypeData::Borrow { kind, target } => TypeData::Borrow {
                kind: *kind,
                target: self.substitute_type_data(*target, substitution)?,
            },
            TypeData::TraitView(application) => {
                let application = self.trait_application_data(*application)?;
                let nested =
                    self.substitute_generic_substitution(application.substitution(), substitution)?;
                let application = self.intern_trait_application(TraitApplicationData::new(
                    application.definition(),
                    nested,
                ))?;

                TypeData::TraitView(application)
            }
            TypeData::OwnedIndirection { storage, target } => TypeData::OwnedIndirection {
                storage: self.substitute_type_data(*storage, substitution)?,
                target: self.substitute_type_data(*target, substitution)?,
            },
            TypeData::Callable(callable) => {
                let parameters = callable
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        Ok(CallableParameterData::new(
                            parameter.name().clone(),
                            parameter.position(),
                            parameter.mode(),
                            self.substitute_type_data(parameter.ty(), substitution)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

                TypeData::Callable(CallableTypeData::new(
                    parameters,
                    self.substitute_type_data(callable.result(), substitution)?,
                    callable.constness(),
                    callable.trust(),
                    callable.abi(),
                    callable.dependency_contracts(),
                ))
            }
        };

        self.intern_type(substituted)
    }

    fn substitute_generic_substitution(
        &self,
        nested: GenericSubstitutionId,
        substitution: &GenericSubstitutionData,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let nested = self.generic_substitution_data(nested)?;

        let arguments = nested
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => self
                    .substitute_type_data(ty, substitution)
                    .map(GenericArgument::Type),
                GenericArgument::Constant(term) => self
                    .substitute_constant_term(term, substitution)
                    .map(GenericArgument::Constant),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let substituted = GenericSubstitutionData::try_new(
            nested.owner(),
            nested.bindings().iter().map(|binding| binding.parameter()),
            arguments,
        )
        .map_err(|_| SemanticValueStoreError::OpenSubstitution)?;

        self.intern_generic_substitution(substituted)
    }

    fn substitute_constant_term(
        &self,
        term: ConstantTermId,
        substitution: &GenericSubstitutionData,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        let data = self.constant_term_data(term)?;

        if let ConstantTermData::Parameter(parameter) = data.as_ref()
            && let Some(GenericArgument::Constant(argument)) =
                substitution.argument_for(GenericParameterSymbolId::Const(*parameter))
        {
            return Ok(argument);
        }

        let substituted = match data.as_ref() {
            ConstantTermData::Value(_)
            | ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::Parameter(_)
            | ConstantTermData::TargetFact(_) => return Ok(term),
            ConstantTermData::Unary { operation, operand } => ConstantTermData::Unary {
                operation: *operation,
                operand: self.substitute_constant_term(*operand, substitution)?,
            },
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => ConstantTermData::Binary {
                operation: *operation,
                left: self.substitute_constant_term(*left, substitution)?,
                right: self.substitute_constant_term(*right, substitution)?,
            },
            ConstantTermData::DefinitionApplication {
                definition,
                substitution: nested,
                selected_implementation,
            } => {
                let selected_implementation = selected_implementation
                    .map(|implementation| {
                        self.substitute_implementation_instance(implementation, substitution)
                    })
                    .transpose()?;

                ConstantTermData::DefinitionApplication {
                    definition: *definition,
                    substitution: self.substitute_generic_substitution(*nested, substitution)?,
                    selected_implementation,
                }
            }
            ConstantTermData::Call {
                callable,
                arguments,
            } => ConstantTermData::call(
                self.substitute_callable_instance(*callable, substitution)?,
                arguments
                    .iter()
                    .map(|argument| self.substitute_constant_term(*argument, substitution))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantTermData::Projection(projection) => {
                let kind = match projection.kind() {
                    ConstantProjectionKind::ArrayElement(index) => {
                        ConstantProjectionKind::ArrayElement(
                            self.substitute_constant_term(index, substitution)?,
                        )
                    }
                    kind => kind,
                };

                ConstantTermData::Projection(ConstantProjection::new(
                    self.substitute_constant_term(projection.subject(), substitution)?,
                    kind,
                ))
            }
        };

        self.intern_constant_term(substituted)
    }

    fn substitute_callable_instance(
        &self,
        callable: super::CallableInstanceId,
        substitution: &GenericSubstitutionData,
    ) -> Result<super::CallableInstanceId, SemanticValueStoreError> {
        let callable = self.callable_instance_data(callable)?;
        let nested = self.substitute_generic_substitution(callable.substitution(), substitution)?;

        self.intern_callable_instance(CallableInstanceData::new(callable.definition(), nested))
    }

    fn substitute_implementation_instance(
        &self,
        implementation: super::ImplementationInstanceId,
        substitution: &GenericSubstitutionData,
    ) -> Result<super::ImplementationInstanceId, SemanticValueStoreError> {
        let implementation = self.implementation_instance_data(implementation)?;
        let nested =
            self.substitute_generic_substitution(implementation.substitution(), substitution)?;

        self.intern_implementation_instance(ImplementationInstanceData::new(
            implementation.definition(),
            nested,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConstantTermData, GenericArgument, GenericSubstitutionData, SemanticValueStore, TypeData,
    };
    use crate::{
        AnySymbolId, FunctionSymbolId, GenericConstParameterSymbolId, GenericOwnerId,
        GenericParameterSymbolId, GenericTypeParameterSymbolId, SymbolId,
    };

    #[test]
    fn substitutions_apply_type_and_constant_arguments_through_arrays() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let source_type_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let source_const_parameter =
            GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let target_type_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(4));
        let target_const_parameter =
            GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(5));

        let source_type = store
            .intern_type(TypeData::TypeParameter(source_type_parameter))
            .unwrap_or_else(|error| panic!("source type interning failed: {error:?}"));
        let source_const = store
            .intern_constant_term(ConstantTermData::Parameter(source_const_parameter))
            .unwrap_or_else(|error| panic!("source constant interning failed: {error:?}"));
        let target_type = store
            .intern_type(TypeData::TypeParameter(target_type_parameter))
            .unwrap_or_else(|error| panic!("target type interning failed: {error:?}"));
        let target_const = store
            .intern_constant_term(ConstantTermData::Parameter(target_const_parameter))
            .unwrap_or_else(|error| panic!("target constant interning failed: {error:?}"));

        let subject = store
            .intern_type(TypeData::Array {
                element: source_type,
                length: source_const,
            })
            .unwrap_or_else(|error| panic!("subject type interning failed: {error:?}"));

        let function = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));
        let owner = GenericOwnerId::try_new(function)
            .unwrap_or_else(|| panic!("function must support generic substitutions"));
        let substitution = GenericSubstitutionData::try_new(
            owner,
            [
                GenericParameterSymbolId::Type(source_type_parameter),
                GenericParameterSymbolId::Const(source_const_parameter),
            ],
            [
                GenericArgument::Type(target_type),
                GenericArgument::Constant(target_const),
            ],
        )
        .unwrap_or_else(|error| panic!("substitution construction failed: {error:?}"));
        let substitution = store
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("substitution interning failed: {error:?}"));

        let substituted = store
            .substitute_type(subject, substitution)
            .unwrap_or_else(|error| panic!("type substitution failed: {error:?}"));
        let substituted = store
            .type_data(substituted)
            .unwrap_or_else(|error| panic!("substituted type must be available: {error:?}"));

        assert_eq!(
            substituted.as_ref(),
            &TypeData::Array {
                element: target_type,
                length: target_const,
            }
        );
    }
}
