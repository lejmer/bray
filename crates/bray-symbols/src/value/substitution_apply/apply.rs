use std::collections::BTreeMap;

use super::super::{
    CallableInstanceData, CallableParameterData, CallableTypeData, ConstantField, ConstantTermData,
    ConstantTermId, ConstantValueData, ConstantValueId, ConstantValueKind, GenericArgument,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData, SemanticValueStore,
    SemanticValueStoreError, TraitApplicationData, TypeData, TypeId,
};
use crate::{
    GenericOwnerId, GenericParameterSymbolId, SelfTypeContext, StaticInstanceKey,
    StaticReferenceSelection,
};

impl SemanticValueStore {
    /// Applies one generic substitution throughout a canonical semantic type.
    pub fn substitute_type(
        &self,
        ty: TypeId,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_type_data(ty, &substitution)
    }

    /// Applies one generic substitution throughout a canonical constant term.
    pub fn substitute_constant_term(
        &self,
        term: ConstantTermId,
        substitution: GenericSubstitutionId,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_constant_term_data(term, &substitution)
    }

    /// Applies one generic substitution throughout a materializable constant value.
    pub fn substitute_constant_value(
        &self,
        value: ConstantValueId,
        substitution: GenericSubstitutionId,
    ) -> Result<ConstantValueId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);
        let mut substituted = BTreeMap::new();

        self.substitute_constant_value_data(value, &substitution, &mut substituted)
    }

    /// Applies one generic substitution throughout another generic substitution.
    pub fn substitute_generic_substitution(
        &self,
        nested: GenericSubstitutionId,
        substitution: GenericSubstitutionId,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_generic_substitution_data(nested, &substitution)
    }

    /// Applies one generic substitution to an exact callable instance.
    pub fn substitute_callable_instance(
        &self,
        callable: super::super::CallableInstanceId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::super::CallableInstanceId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_callable_instance_data(callable, &substitution)
    }

    /// Applies one generic substitution to an exact implementation witness.
    pub fn substitute_implementation_instance(
        &self,
        implementation: super::super::ImplementationInstanceId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::super::ImplementationInstanceId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_implementation_instance_data(implementation, &substitution)
    }

    /// Applies one generic substitution throughout a trait application.
    pub fn substitute_trait_application(
        &self,
        application: super::super::TraitApplicationId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::super::TraitApplicationId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_trait_application_data(application, &substitution)
    }

    pub(super) fn substitute_trait_application_data(
        &self,
        application: super::super::TraitApplicationId,
        substitution: &GenericSubstitutionData,
    ) -> Result<super::super::TraitApplicationId, SemanticValueStoreError> {
        let application = self.trait_application_data(application);

        let nested =
            self.substitute_generic_substitution_data(application.substitution(), substitution)?;

        self.intern_trait_application(TraitApplicationData::new(application.definition(), nested))
    }

    pub(super) fn substitute_type_data(
        &self,
        ty: TypeId,
        substitution: &GenericSubstitutionData,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let data = self.type_data(ty);

        if let TypeData::TypeParameter(parameter) = data.as_ref()
            && let Some(GenericArgument::Type(argument)) =
                substitution.argument_for(GenericParameterSymbolId::Type(*parameter))
        {
            return Ok(argument);
        }

        let substituted = match data.as_ref() {
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
                    return Ok(ty);
                };

                TypeData::Named {
                    definition: *definition,
                    substitution: self
                        .intern_generic_substitution(substitution.with_owner(owner))?,
                }
            }
            TypeData::ExactInteger { .. }
            | TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_) => {
                return Ok(ty);
            }
            TypeData::Named {
                definition,
                substitution: nested,
            } => TypeData::Named {
                definition: *definition,
                substitution: self.substitute_generic_substitution_data(*nested, substitution)?,
            },
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                let application = self.trait_application_data(*application);

                let nested = self.substitute_generic_substitution_data(
                    application.substitution(),
                    substitution,
                )?;

                let application = self.intern_trait_application(TraitApplicationData::new(
                    application.definition(),
                    nested,
                ))?;

                TypeData::TypeValuedMemberProjection {
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
                length: self.substitute_constant_term_data(*length, substitution)?,
            },
            TypeData::FlexibleArray(element) => {
                TypeData::FlexibleArray(self.substitute_type_data(*element, substitution)?)
            }
            TypeData::Slice(element) => {
                TypeData::Slice(self.substitute_type_data(*element, substitution)?)
            }
            TypeData::Generator(element) => {
                TypeData::Generator(self.substitute_type_data(*element, substitution)?)
            }
            TypeData::Nullable(target) => {
                TypeData::Nullable(self.substitute_type_data(*target, substitution)?)
            }
            TypeData::Borrow { kind, target } => TypeData::Borrow {
                kind: *kind,
                target: self.substitute_type_data(*target, substitution)?,
            },
            TypeData::TraitView(application) => {
                let application = self.trait_application_data(*application);

                let nested = self.substitute_generic_substitution_data(
                    application.substitution(),
                    substitution,
                )?;

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

                let dependencies = self.substitute_callable_dependency_contracts(
                    callable.dependency_contracts(),
                    substitution,
                )?;

                let phase_behaviors = callable
                    .phase_behaviors()
                    .try_with_dependency_contracts(dependencies)
                    .expect("substitution preserves callable phase contracts");

                let body = callable
                    .phase_behaviors()
                    .deferred_execution()
                    .unwrap_or_else(|| callable.phase_behaviors().invocation());

                let substitute_predicate = |predicate: &crate::PredicateSemanticSummary| -> Result<_, SemanticValueStoreError> {
                    Ok(crate::PredicateSemanticSummary::new(self.substitute_dependency_contract_data(predicate.dependency_contract(), substitution)?)
                        .with_condition(predicate.condition().map(|term| self.substitute_constant_term_data(term, substitution)).transpose()?, predicate.domain()))
                };

                let phase_behaviors = phase_behaviors.with_receiver_cleanup(
                    body.receiver_cleanup()
                        .iter()
                        .map(|ty| self.substitute_type_data(*ty, substitution))
                        .collect::<Result<Vec<_>, _>>()?,
                );

                let phase_behaviors = phase_behaviors.with_predicates(
                    callable
                        .phase_behaviors()
                        .invocation()
                        .predicate_requirements()
                        .iter()
                        .map(substitute_predicate)
                        .collect::<Result<Vec<_>, _>>()?,
                    body.predicate_guarantees()
                        .iter()
                        .map(substitute_predicate)
                        .collect::<Result<Vec<_>, _>>()?,
                );

                TypeData::Callable(
                    CallableTypeData::new(
                        parameters,
                        self.substitute_type_data(callable.result(), substitution)?,
                        callable.constness(),
                        callable.trust(),
                        callable.abi(),
                        dependencies,
                    )
                    .with_variadic(callable.is_variadic())
                    .with_phase_behaviors(phase_behaviors),
                )
            }
        };

        self.intern_type(substituted)
    }

    fn substitute_generic_substitution_data(
        &self,
        nested: GenericSubstitutionId,
        substitution: &GenericSubstitutionData,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let nested = self.generic_substitution_data(nested);

        let arguments = nested
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => self
                    .substitute_type_data(ty, substitution)
                    .map(GenericArgument::Type),
                GenericArgument::Constant(term) => self
                    .substitute_constant_term_data(term, substitution)
                    .map(GenericArgument::Constant),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let substituted = GenericSubstitutionData::try_new(
            nested.owner(),
            nested.bindings().iter().map(|binding| binding.parameter()),
            arguments,
        )
        .expect("substitution preserves parameter kinds and arity");

        self.intern_generic_substitution(substituted)
    }

    pub(super) fn substitute_constant_term_data(
        &self,
        term: ConstantTermId,
        substitution: &GenericSubstitutionData,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        let data = self.constant_term_data(term);

        if let ConstantTermData::Parameter(parameter) = data.as_ref()
            && let Some(GenericArgument::Constant(argument)) =
                substitution.argument_for(GenericParameterSymbolId::Const(*parameter))
        {
            return Ok(argument);
        }

        let substituted = match data.as_ref() {
            ConstantTermData::Typed { term, ty } => ConstantTermData::typed(
                self.substitute_constant_term_data(*term, substitution)?,
                self.substitute_type_data(*ty, substitution)?,
            ),
            ConstantTermData::Value(value) => ConstantTermData::Value(
                self.substitute_constant_value_data(*value, substitution, &mut BTreeMap::new())?,
            ),
            ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::CallableArgument(_)
            | ConstantTermData::Parameter(_)
            | ConstantTermData::TargetProperty(_) => return Ok(term),
            ConstantTermData::Unary { operation, operand } => ConstantTermData::Unary {
                operation: *operation,
                operand: self.substitute_constant_term_data(*operand, substitution)?,
            },
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => ConstantTermData::Binary {
                operation: *operation,
                left: self.substitute_constant_term_data(*left, substitution)?,
                right: self.substitute_constant_term_data(*right, substitution)?,
            },
            ConstantTermData::Conversion { operand, target } => ConstantTermData::Conversion {
                operand: self.substitute_constant_term_data(*operand, substitution)?,
                target: self.substitute_type_data(*target, substitution)?,
            },
            ConstantTermData::VariantTest { subject, variant } => ConstantTermData::VariantTest {
                subject: self.substitute_constant_term_data(*subject, substitution)?,
                variant: *variant,
            },
            ConstantTermData::NullablePresent(value) => ConstantTermData::NullablePresent(
                self.substitute_constant_term_data(*value, substitution)?,
            ),
            ConstantTermData::Tuple(values) => ConstantTermData::tuple(
                values
                    .iter()
                    .map(|value| self.substitute_constant_term_data(*value, substitution))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantTermData::Array(values) => ConstantTermData::array(
                values
                    .iter()
                    .map(|value| self.substitute_constant_term_data(*value, substitution))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantTermData::Product(fields) => ConstantTermData::product(
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            *field.field(),
                            self.substitute_constant_term_data(*field.value(), substitution)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, SemanticValueStoreError>>()?,
            ),
            ConstantTermData::Union { variant, fields } => ConstantTermData::union(
                *variant,
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            *field.field(),
                            self.substitute_constant_term_data(*field.value(), substitution)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, SemanticValueStoreError>>()?,
            ),
            ConstantTermData::DefinitionApplication {
                definition,
                substitution: nested,
                selected_implementation,
            } => {
                let selected_implementation = selected_implementation
                    .map(|implementation| {
                        self.substitute_implementation_instance_data(implementation, substitution)
                    })
                    .transpose()?;

                ConstantTermData::DefinitionApplication {
                    definition: *definition,
                    substitution: self
                        .substitute_generic_substitution_data(*nested, substitution)?,
                    selected_implementation,
                }
            }
            ConstantTermData::Call {
                callable,
                selected_implementation,
                arguments,
            } => {
                let selected_implementation = selected_implementation
                    .map(|implementation| {
                        self.substitute_implementation_instance_data(implementation, substitution)
                    })
                    .transpose()?;

                ConstantTermData::call(
                    self.substitute_callable_instance_data(*callable, substitution)?,
                    selected_implementation,
                    arguments
                        .iter()
                        .map(|argument| self.substitute_constant_term_data(*argument, substitution))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }
            ConstantTermData::PredicateCall {
                predicate,
                arguments,
            } => ConstantTermData::predicate_call(
                crate::PredicateInstanceData::new(
                    predicate.definition(),
                    self.substitute_generic_substitution_data(
                        predicate.substitution(),
                        substitution,
                    )?,
                ),
                arguments
                    .iter()
                    .map(|argument| self.substitute_constant_term_data(*argument, substitution))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantTermData::Projection(projection) => ConstantTermData::Projection(
                self.substitute_constant_projection(*projection, substitution)?,
            ),
        };

        self.intern_constant_term(substituted)
    }

    fn substitute_constant_value_data(
        &self,
        value: ConstantValueId,
        substitution: &GenericSubstitutionData,
        substituted: &mut BTreeMap<ConstantValueId, ConstantValueId>,
    ) -> Result<ConstantValueId, SemanticValueStoreError> {
        if let Some(value) = substituted.get(&value) {
            return Ok(*value);
        }

        let source = value;
        let data = self.constant_value_data(value);
        let ty = self.substitute_type_data(data.ty(), substitution)?;

        let kind = match data.kind() {
            ConstantValueKind::Error => ConstantValueKind::Error,
            ConstantValueKind::Boolean(value) => ConstantValueKind::Boolean(*value),
            ConstantValueKind::Character(value) => ConstantValueKind::Character(*value),
            ConstantValueKind::Integer(value) => ConstantValueKind::Integer(value.clone()),
            ConstantValueKind::Real(value) => ConstantValueKind::Real(*value),
            ConstantValueKind::Complex { real, imaginary } => ConstantValueKind::Complex {
                real: *real,
                imaginary: *imaginary,
            },
            ConstantValueKind::String(value) => ConstantValueKind::String(value.clone()),
            ConstantValueKind::StaticAddress(selection) => ConstantValueKind::StaticAddress(
                self.substitute_static_reference(selection, substitution)?,
            ),
            ConstantValueKind::Borrowed(value) => ConstantValueKind::Borrowed(
                self.substitute_constant_value_data(*value, substitution, substituted)?,
            ),
            ConstantValueKind::Unit => ConstantValueKind::Unit,
            ConstantValueKind::NullableAbsent => ConstantValueKind::NullableAbsent,
            ConstantValueKind::NullablePresent(value) => ConstantValueKind::NullablePresent(
                self.substitute_constant_value_data(*value, substitution, substituted)?,
            ),
            ConstantValueKind::Tuple(values) => ConstantValueKind::tuple(
                values
                    .iter()
                    .copied()
                    .map(|value| {
                        self.substitute_constant_value_data(value, substitution, substituted)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantValueKind::Array(values) => ConstantValueKind::array(
                values
                    .iter()
                    .copied()
                    .map(|value| {
                        self.substitute_constant_value_data(value, substitution, substituted)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ConstantValueKind::Product(fields) => ConstantValueKind::product(
                self.substitute_constant_fields(fields, substitution, substituted)?,
            ),
            ConstantValueKind::Union { variant, fields } => ConstantValueKind::union(
                *variant,
                self.substitute_constant_fields(fields, substitution, substituted)?,
            ),
        };

        let value = self.intern_constant_value(ConstantValueData::new(ty, kind))?;

        substituted.insert(source, value);

        Ok(value)
    }

    fn substitute_constant_fields<I>(
        &self,
        fields: &[ConstantField<I, ConstantValueId>],
        substitution: &GenericSubstitutionData,
        substituted: &mut BTreeMap<ConstantValueId, ConstantValueId>,
    ) -> Result<Vec<ConstantField<I, ConstantValueId>>, SemanticValueStoreError>
    where
        I: Copy,
    {
        fields
            .iter()
            .map(|field| {
                Ok(ConstantField::new(
                    *field.field(),
                    self.substitute_constant_value_data(*field.value(), substitution, substituted)?,
                ))
            })
            .collect()
    }

    fn substitute_static_reference(
        &self,
        selection: &StaticReferenceSelection,
        substitution: &GenericSubstitutionData,
    ) -> Result<StaticReferenceSelection, SemanticValueStoreError> {
        let StaticReferenceSelection::Open {
            template,
            substitution: nested,
            selected_witnesses,
            target,
        } = selection
        else {
            return Ok(selection.clone());
        };

        let nested = self.substitute_generic_substitution_data(*nested, substitution)?;

        let selected_witnesses = selected_witnesses
            .iter()
            .copied()
            .map(|witness| self.substitute_implementation_instance_data(witness, substitution))
            .collect::<Result<Vec<_>, _>>()?;

        match self.substitution_is_concrete(nested) {
            true => Ok(StaticReferenceSelection::Closed(StaticInstanceKey::new(
                *template,
                nested,
                selected_witnesses,
                target.clone(),
            ))),
            false => Ok(StaticReferenceSelection::open(
                *template,
                nested,
                selected_witnesses,
                target.clone(),
            )),
        }
    }

    pub(super) fn substitute_callable_instance_data(
        &self,
        callable: super::super::CallableInstanceId,
        substitution: &GenericSubstitutionData,
    ) -> Result<super::super::CallableInstanceId, SemanticValueStoreError> {
        let callable = self.callable_instance_data(callable);

        let nested =
            self.substitute_generic_substitution_data(callable.substitution(), substitution)?;

        self.intern_callable_instance(CallableInstanceData::new(callable.definition(), nested))
    }

    pub(super) fn substitute_implementation_instance_data(
        &self,
        implementation: super::super::ImplementationInstanceId,
        substitution: &GenericSubstitutionData,
    ) -> Result<super::super::ImplementationInstanceId, SemanticValueStoreError> {
        let implementation = self.implementation_instance_data(implementation);

        let nested =
            self.substitute_generic_substitution_data(implementation.substitution(), substitution)?;

        self.intern_implementation_instance(ImplementationInstanceData::new(
            implementation.definition(),
            nested,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        AnySymbolId, CallableAbi, CallableConstness, CallableTrust, DependencyRequirementKind,
        FunctionSymbolId, GenericConstParameterSymbolId, GenericOwnerId, GenericParameterSymbolId,
        GenericTypeParameterSymbolId, SymbolId, SymbolOrdinal,
    };
    use crate::{
        CallableDependencyContracts, CallableTypeData, ConstantTermData, ConstantValueData,
        ConstantValueKind, DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
        GenericSubstitutionData, SemanticValueStore, TypeData,
    };

    #[test]
    fn dependency_parameter_occurrences_include_types_nested_in_constants() {
        let store = SemanticValueStore::try_new().unwrap();
        let ty_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let unused = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let count = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(4));

        let owner = GenericOwnerId::try_new(AnySymbolId::from(FunctionSymbolId::from_symbol_id(
            SymbolId::new(1),
        )))
        .unwrap();

        let ty = store
            .intern_type(TypeData::TypeParameter(ty_parameter))
            .unwrap();

        let value = store
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Error))
            .unwrap();

        let value = store
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap();

        let count_term = store
            .intern_constant_term(ConstantTermData::Parameter(count))
            .unwrap();

        let template = store
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::guarded(
                    DependencyGuard::NullablePresent(DependencySubject::root(
                        DependencySubjectRoot::Receiver,
                    )),
                    [DependencyRequirement::direct(
                        DependencySubject::new(
                            DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
                            [
                                DependencyProjection::Element(value),
                                DependencyProjection::Element(count_term),
                            ],
                        ),
                        DependencyRequirementKind::ValueDependencies,
                    )],
                ),
            ]))
            .unwrap();

        for (parameter, expected) in [
            (GenericParameterSymbolId::Type(ty_parameter), true),
            (GenericParameterSymbolId::Const(count), true),
            (GenericParameterSymbolId::Type(unused), false),
        ] {
            assert_eq!(
                store
                    .dependency_contract_uses_parameter(template, owner, parameter)
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn substitutions_apply_to_types_retained_by_constant_values() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));

        let source_type = store
            .intern_type(TypeData::TypeParameter(parameter))
            .unwrap_or_else(|error| panic!("source type interning failed: {error:?}"));

        let target_type = store
            .intern_type(TypeData::Tuple(Arc::from([])))
            .unwrap_or_else(|error| panic!("target type interning failed: {error:?}"));

        let value = store
            .intern_constant_value(ConstantValueData::new(
                source_type,
                ConstantValueKind::Error,
            ))
            .unwrap_or_else(|error| panic!("constant value interning failed: {error:?}"));

        let term = store
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("constant term interning failed: {error:?}"));

        let owner = GenericOwnerId::try_new(AnySymbolId::from(FunctionSymbolId::from_symbol_id(
            SymbolId::new(1),
        )))
        .unwrap_or_else(|| panic!("function must support generic substitutions"));

        let substitution = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Type(parameter)],
            [GenericArgument::Type(target_type)],
        )
        .unwrap_or_else(|error| panic!("substitution construction failed: {error:?}"));

        let substitution = store
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("substitution interning failed: {error:?}"));

        let term = store
            .substitute_constant_term(term, substitution)
            .unwrap_or_else(|error| panic!("constant substitution failed: {error:?}"));

        let term = store.constant_term_data(term);

        let ConstantTermData::Value(value) = term.as_ref() else {
            panic!("substituted term must remain a value");
        };

        let value = store.constant_value_data(*value);

        assert_eq!(value.ty(), target_type);
        assert_eq!(value.kind(), &ConstantValueKind::Error);
    }

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

        let substituted = store.type_data(substituted);

        assert_eq!(
            substituted.as_ref(),
            &TypeData::Array {
                element: target_type,
                length: target_const,
            }
        );
    }

    #[test]
    fn substitutions_apply_to_open_scalar_conversions() {
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

        let conversion = store
            .intern_constant_term(ConstantTermData::Conversion {
                operand: source_const,
                target: source_type,
            })
            .unwrap_or_else(|error| panic!("conversion interning failed: {error:?}"));

        let owner = GenericOwnerId::try_new(AnySymbolId::from(FunctionSymbolId::from_symbol_id(
            SymbolId::new(1),
        )))
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

        let substituted = store
            .substitute_constant_term_data(conversion, &substitution)
            .unwrap_or_else(|error| panic!("constant substitution failed: {error:?}"));

        let substituted = store.constant_term_data(substituted);

        assert_eq!(
            substituted.as_ref(),
            &ConstantTermData::Conversion {
                operand: target_const,
                target: target_type,
            }
        );
    }

    #[test]
    fn substitutions_apply_constant_arguments_through_callable_dependency_contracts() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let source_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let target_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(3));

        let source_term = store
            .intern_constant_term(ConstantTermData::Parameter(source_parameter))
            .unwrap_or_else(|error| panic!("source constant interning failed: {error:?}"));

        let target_term = store
            .intern_constant_term(ConstantTermData::Parameter(target_parameter))
            .unwrap_or_else(|error| panic!("target constant interning failed: {error:?}"));

        let source_subject = DependencySubject::new(
            DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
            [DependencyProjection::Element(source_term)],
        );

        let target_subject = DependencySubject::new(
            DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
            [DependencyProjection::Element(target_term)],
        );

        let projected_reference = |index| {
            crate::DependencyBorrowRelation::new(
                DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0))),
                DependencySubject::root(DependencySubjectRoot::Result),
                crate::DependencyReferenceKind::Borrow,
            )
            .with_target_projections([DependencyProjection::Element(index)])
        };

        let invocation = store
            .intern_dependency_contract_template(
                DependencyContractTemplateData::new([DependencyRequirement::direct(
                    source_subject.clone(),
                    DependencyRequirementKind::StorageAlive,
                )])
                .with_borrows([projected_reference(source_term)]),
            )
            .unwrap_or_else(|error| panic!("invocation contract interning failed: {error:?}"));

        let deferred = store
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::guarded(
                    DependencyGuard::NullablePresent(source_subject),
                    [DependencyRequirement::direct(
                        DependencySubject::root(DependencySubjectRoot::Result),
                        DependencyRequirementKind::StorageInitialized,
                    )],
                ),
            ]))
            .unwrap_or_else(|error| panic!("deferred contract interning failed: {error:?}"));

        let result = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("callable result interning failed: {error:?}"));

        let subject = store
            .intern_type(TypeData::Callable(CallableTypeData::new(
                [],
                result,
                CallableConstness::Runtime,
                CallableTrust::Safe,
                CallableAbi::Bray,
                CallableDependencyContracts::asynchronous(invocation, deferred),
            )))
            .unwrap_or_else(|error| panic!("callable type interning failed: {error:?}"));

        let function = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));

        let owner = GenericOwnerId::try_new(function)
            .unwrap_or_else(|| panic!("function must support generic substitutions"));

        let substitution = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Const(source_parameter)],
            [GenericArgument::Constant(target_term)],
        )
        .unwrap_or_else(|error| panic!("substitution construction failed: {error:?}"));

        let substitution = store
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("substitution interning failed: {error:?}"));

        let substituted = store
            .substitute_type(subject, substitution)
            .unwrap_or_else(|error| panic!("type substitution failed: {error:?}"));

        let substituted = store.type_data(substituted);

        let TypeData::Callable(callable) = substituted.as_ref() else {
            panic!("substituted type must remain callable");
        };

        let contracts = callable.dependency_contracts();

        let invocation = store.dependency_contract_template_data(contracts.invocation());

        let deferred = contracts
            .deferred_execution()
            .unwrap_or_else(|| panic!("substituted callable must remain asynchronous"));

        let deferred = store.dependency_contract_template_data(deferred);

        assert_eq!(
            invocation.as_ref(),
            &DependencyContractTemplateData::new([DependencyRequirement::direct(
                target_subject.clone(),
                DependencyRequirementKind::StorageAlive,
            )])
            .with_borrows([projected_reference(target_term)])
        );

        assert_eq!(
            deferred.as_ref(),
            &DependencyContractTemplateData::new([DependencyRequirement::guarded(
                DependencyGuard::NullablePresent(target_subject),
                [DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Result),
                    DependencyRequirementKind::StorageInitialized,
                )],
            )])
        );
    }
}
