use std::collections::BTreeMap;

use super::{
    CallableDependencyContracts, CallableInstanceData, CallableParameterData, CallableTypeData,
    ConstantField, ConstantTermData, ConstantTermId, ConstantValueData, ConstantValueId,
    ConstantValueKind, DependencyContractTemplateData, DependencyGuard, DependencyProjection,
    DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData, SemanticValueStore,
    SemanticValueStoreError, TraitApplicationData, TypeData, TypeId,
};
use crate::{
    CallableConditionSet, CallableConditions, CallableContractClause, CallableContractClauseValue,
    GenericOwnerId, GenericParameterSymbolId, PredicateSemanticSummary, SelfTypeContext,
    StaticInstanceKey, StaticReferenceSelection,
};

pub(super) trait SemanticSubstitution {
    fn replacement_type(
        &self,
        values: &SemanticValueStore,
        data: &TypeData,
    ) -> Result<Option<TypeId>, SemanticValueStoreError>;

    fn replacement_constant(
        &self,
        _parameter: crate::GenericConstParameterSymbolId,
    ) -> Option<ConstantTermId> {
        None
    }
}

impl SemanticSubstitution for GenericSubstitutionData {
    fn replacement_type(
        &self,
        values: &SemanticValueStore,
        data: &TypeData,
    ) -> Result<Option<TypeId>, SemanticValueStoreError> {
        match data {
            TypeData::TypeParameter(parameter) => Ok(
                match self.argument_for(GenericParameterSymbolId::Type(*parameter)) {
                    Some(GenericArgument::Type(argument)) => Some(argument),
                    _ => None,
                },
            ),
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
                    return Ok(None);
                };

                let substitution = values.intern_generic_substitution(self.with_owner(owner))?;

                values
                    .intern_type(TypeData::Named {
                        definition: *definition,
                        substitution,
                    })
                    .map(Some)
            }
            _ => Ok(None),
        }
    }

    fn replacement_constant(
        &self,
        parameter: crate::GenericConstParameterSymbolId,
    ) -> Option<ConstantTermId> {
        match self.argument_for(GenericParameterSymbolId::Const(parameter)) {
            Some(GenericArgument::Constant(argument)) => Some(argument),
            _ => None,
        }
    }
}

impl SemanticValueStore {
    /// Applies generic arguments throughout callable predicates and dependency contracts.
    pub fn substitute_callable_conditions(
        &self,
        conditions: &CallableConditionSet,
        substitution: GenericSubstitutionId,
    ) -> Result<CallableConditionSet, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;

        self.substitute_conditions_data(conditions, substitution.as_ref())
    }

    /// Applies one generic substitution throughout a canonical semantic type.
    pub fn substitute_type(
        &self,
        ty: TypeId,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;

        self.substitute_type_data(ty, substitution.as_ref())
    }

    /// Applies one generic substitution throughout a dependency contract.
    pub fn substitute_dependency_contract(
        &self,
        contract: super::DependencyContractTemplateId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::DependencyContractTemplateId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;

        self.substitute_dependency_contract_data(contract, substitution.as_ref())
    }

    /// Applies one generic substitution throughout a canonical constant term.
    pub fn substitute_constant_term(
        &self,
        term: ConstantTermId,
        substitution: GenericSubstitutionId,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;

        self.substitute_constant_term_data(term, substitution.as_ref())
    }

    /// Applies one generic substitution throughout a materializable constant value.
    pub fn substitute_constant_value(
        &self,
        value: ConstantValueId,
        substitution: GenericSubstitutionId,
    ) -> Result<ConstantValueId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;
        let mut substituted = BTreeMap::new();

        self.substitute_constant_value_data(value, substitution.as_ref(), &mut substituted)
    }

    /// Applies one generic substitution throughout another generic substitution.
    pub fn substitute_generic_substitution(
        &self,
        nested: GenericSubstitutionId,
        substitution: GenericSubstitutionId,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution)?;

        self.substitute_generic_substitution_data(nested, substitution.as_ref())
    }

    /// Applies one generic substitution throughout a trait application.
    pub fn substitute_trait_application(
        &self,
        application: super::TraitApplicationId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::TraitApplicationId, SemanticValueStoreError> {
        let application = self.trait_application_data(application)?;

        let nested =
            self.substitute_generic_substitution(application.substitution(), substitution)?;

        self.intern_trait_application(TraitApplicationData::new(application.definition(), nested))
    }

    pub(super) fn substitute_type_data(
        &self,
        ty: TypeId,
        substitution: &impl SemanticSubstitution,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let data = self.type_data(ty)?;

        if let Some(replacement) = substitution.replacement_type(self, &data)? {
            return Ok(replacement);
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
                substitution: self.substitute_generic_substitution_data(*nested, substitution)?,
            },
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                let application = self.trait_application_data(*application)?;

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
                let application = self.trait_application_data(*application)?;

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
                    .ok_or(SemanticValueStoreError::OpenSubstitution)?;

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
                    .with_conditions(
                        self.substitute_conditions_data(callable.conditions(), substitution)?,
                    )
                    .with_phase_behaviors(phase_behaviors),
                )
            }
        };

        self.intern_type(substituted)
    }

    fn substitute_conditions_data(
        &self,
        conditions: &CallableConditionSet,
        substitution: &impl SemanticSubstitution,
    ) -> Result<CallableConditionSet, SemanticValueStoreError> {
        // Share immutable clause groups until the complete substitution succeeds.
        let mut conditions = conditions.clone();

        conditions.try_map_clauses(|clause| {
            let mapped = match clause.value() {
                CallableContractClauseValue::Predicate(predicate) => {
                    let dependency = self.substitute_dependency_contract_data(
                        predicate.dependency_contract(),
                        substitution,
                    )?;

                    let condition = predicate
                        .condition()
                        .map(|term| self.substitute_constant_term_data(term, substitution))
                        .transpose()?;

                    CallableContractClause::new(
                        clause.ordinal(),
                        clause.kind(),
                        PredicateSemanticSummary::new(dependency).with_condition(condition),
                    )
                }
                CallableContractClauseValue::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    let application = self.trait_application_data(application)?;

                    let nested = self.substitute_generic_substitution_data(
                        application.substitution(),
                        substitution,
                    )?;

                    let application = self.intern_trait_application(TraitApplicationData::new(
                        application.definition(),
                        nested,
                    ))?;

                    CallableContractClause::trait_satisfaction(
                        clause.ordinal(),
                        self.substitute_type_data(subject, substitution)?,
                        application,
                    )
                }
            };

            Ok(mapped.with_guard(clause.guard()))
        })?;

        Ok(conditions)
    }

    pub(super) fn substitute_generic_substitution_data(
        &self,
        nested: GenericSubstitutionId,
        substitution: &impl SemanticSubstitution,
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
                    .substitute_constant_term_data(term, substitution)
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

    pub(super) fn substitute_constant_term_data(
        &self,
        term: ConstantTermId,
        substitution: &impl SemanticSubstitution,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        let data = self.constant_term_data(term)?;

        if let ConstantTermData::Parameter(parameter) = data.as_ref()
            && let Some(argument) = substitution.replacement_constant(*parameter)
        {
            return Ok(argument);
        }

        let mut substituted =
            data.try_map_terms(|child| self.substitute_constant_term_data(child, substitution))?;

        match &mut substituted {
            ConstantTermData::Typed { ty, .. } => {
                *ty = self.substitute_type_data(*ty, substitution)?;
            }
            ConstantTermData::Value(value) => {
                *value = self.substitute_constant_value_data(
                    *value,
                    substitution,
                    &mut BTreeMap::new(),
                )?;
            }
            ConstantTermData::Conversion { target, .. } => {
                *target = self.substitute_type_data(*target, substitution)?;
            }
            ConstantTermData::DefinitionApplication {
                substitution: nested,
                selected_implementation,
                ..
            } => {
                *nested = self.substitute_generic_substitution_data(*nested, substitution)?;

                *selected_implementation = selected_implementation
                    .map(|instance| self.substitute_implementation_instance(instance, substitution))
                    .transpose()?;
            }
            ConstantTermData::Call {
                callable,
                selected_implementation,
                ..
            } => {
                *callable = self.substitute_callable_instance(*callable, substitution)?;

                *selected_implementation = selected_implementation
                    .map(|instance| self.substitute_implementation_instance(instance, substitution))
                    .transpose()?;
            }
            ConstantTermData::PredicateCall { predicate, .. } => {
                *predicate = crate::PredicateInstanceData::new(
                    predicate.definition(),
                    self.substitute_generic_substitution_data(
                        predicate.substitution(),
                        substitution,
                    )?,
                );
            }
            ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::CallableArgument(_)
            | ConstantTermData::Parameter(_)
            | ConstantTermData::TargetProperty(_)
            | ConstantTermData::Unary { .. }
            | ConstantTermData::Binary { .. }
            | ConstantTermData::NullablePresent(_)
            | ConstantTermData::Tuple(_)
            | ConstantTermData::Array(_)
            | ConstantTermData::Product(_)
            | ConstantTermData::Union { .. }
            | ConstantTermData::Test { .. }
            | ConstantTermData::Projection(_) => {}
        }

        self.intern_constant_term(substituted)
    }

    fn substitute_constant_value_data(
        &self,
        value: ConstantValueId,
        substitution: &impl SemanticSubstitution,
        substituted: &mut BTreeMap<ConstantValueId, ConstantValueId>,
    ) -> Result<ConstantValueId, SemanticValueStoreError> {
        if let Some(value) = substituted.get(&value) {
            return Ok(*value);
        }

        let source = value;
        let data = self.constant_value_data(value)?;
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
        substitution: &impl SemanticSubstitution,
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
        substitution: &impl SemanticSubstitution,
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
            .map(|witness| self.substitute_implementation_instance(witness, substitution))
            .collect::<Result<Vec<_>, _>>()?;

        match self.require_concrete_substitution(nested) {
            Ok(nested) => Ok(StaticReferenceSelection::Closed(StaticInstanceKey::new(
                *template,
                nested,
                selected_witnesses,
                target.clone(),
            ))),
            Err(_) => Ok(StaticReferenceSelection::open(
                *template,
                nested,
                selected_witnesses,
                target.clone(),
            )),
        }
    }

    fn substitute_callable_dependency_contracts(
        &self,
        contracts: CallableDependencyContracts,
        substitution: &impl SemanticSubstitution,
    ) -> Result<CallableDependencyContracts, SemanticValueStoreError> {
        let invocation =
            self.substitute_dependency_contract_data(contracts.invocation(), substitution)?;

        match contracts.deferred_execution() {
            Some(deferred) => Ok(CallableDependencyContracts::asynchronous(
                invocation,
                self.substitute_dependency_contract_data(deferred, substitution)?,
            )),
            None => Ok(CallableDependencyContracts::synchronous(invocation)),
        }
    }

    fn substitute_dependency_contract_data(
        &self,
        template: super::DependencyContractTemplateId,
        substitution: &impl SemanticSubstitution,
    ) -> Result<super::DependencyContractTemplateId, SemanticValueStoreError> {
        let template = self.dependency_contract_template_data(template)?;

        let requirements = template
            .requirements()
            .iter()
            .map(|requirement| self.substitute_dependency_requirement(requirement, substitution))
            .collect::<Result<Vec<_>, _>>()?;

        self.intern_dependency_contract_template(DependencyContractTemplateData::new(requirements))
    }

    fn substitute_dependency_requirement(
        &self,
        requirement: &DependencyRequirement,
        substitution: &impl SemanticSubstitution,
    ) -> Result<DependencyRequirement, SemanticValueStoreError> {
        match requirement {
            DependencyRequirement::Direct { subject, kind } => Ok(DependencyRequirement::direct(
                self.substitute_dependency_subject(subject, substitution)?,
                *kind,
            )),
            DependencyRequirement::Guarded(guarded) => Ok(DependencyRequirement::guarded(
                self.substitute_dependency_guard(guarded.guard(), substitution)?,
                guarded
                    .requirements()
                    .iter()
                    .map(|requirement| {
                        self.substitute_dependency_requirement(requirement, substitution)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }

    fn substitute_dependency_guard(
        &self,
        guard: &DependencyGuard,
        substitution: &impl SemanticSubstitution,
    ) -> Result<DependencyGuard, SemanticValueStoreError> {
        match guard {
            DependencyGuard::NullablePresent(subject) => Ok(DependencyGuard::NullablePresent(
                self.substitute_dependency_subject(subject, substitution)?,
            )),
            DependencyGuard::ActiveUnionVariant { subject, variant } => {
                Ok(DependencyGuard::ActiveUnionVariant {
                    subject: self.substitute_dependency_subject(subject, substitution)?,
                    variant: *variant,
                })
            }
        }
    }

    fn substitute_dependency_subject(
        &self,
        subject: &DependencySubject,
        substitution: &impl SemanticSubstitution,
    ) -> Result<DependencySubject, SemanticValueStoreError> {
        let root = match subject.subject_root() {
            DependencySubjectRoot::ImplementationWitness(instance) => {
                DependencySubjectRoot::ImplementationWitness(
                    self.substitute_implementation_instance(instance, substitution)?,
                )
            }
            root => root,
        };

        let projections = subject
            .projections()
            .iter()
            .map(|projection| match projection {
                DependencyProjection::Element(index) => self
                    .substitute_constant_term_data(*index, substitution)
                    .map(DependencyProjection::Element),
                projection => Ok(*projection),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DependencySubject::new(root, projections))
    }

    fn substitute_callable_instance(
        &self,
        callable: super::CallableInstanceId,
        substitution: &impl SemanticSubstitution,
    ) -> Result<super::CallableInstanceId, SemanticValueStoreError> {
        let callable = self.callable_instance_data(callable)?;

        let nested =
            self.substitute_generic_substitution_data(callable.substitution(), substitution)?;

        self.intern_callable_instance(CallableInstanceData::new(callable.definition(), nested))
    }

    fn substitute_implementation_instance(
        &self,
        implementation: super::ImplementationInstanceId,
        substitution: &impl SemanticSubstitution,
    ) -> Result<super::ImplementationInstanceId, SemanticValueStoreError> {
        let implementation = self.implementation_instance_data(implementation)?;

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

    use super::{
        CallableDependencyContracts, CallableTypeData, ConstantTermData, ConstantValueData,
        ConstantValueKind, DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
        GenericSubstitutionData, SemanticValueStore, TypeData,
    };
    use crate::{
        AnySymbolId, CallableAbi, CallableConstness, CallableTrust, DependencyRequirementKind,
        FunctionSymbolId, GenericConstParameterSymbolId, GenericOwnerId, GenericParameterSymbolId,
        GenericTypeParameterSymbolId, SymbolId, SymbolOrdinal,
    };

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

        let term = store
            .constant_term_data(term)
            .unwrap_or_else(|error| panic!("substituted term must be available: {error:?}"));

        let ConstantTermData::Value(value) = term.as_ref() else {
            panic!("substituted term must remain a value");
        };

        let value = store
            .constant_value_data(*value)
            .unwrap_or_else(|error| panic!("substituted value must be available: {error:?}"));

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

        let substituted = store
            .constant_term_data(substituted)
            .unwrap_or_else(|error| panic!("substituted term must be available: {error:?}"));

        assert_eq!(
            substituted.as_ref(),
            &ConstantTermData::Conversion {
                operand: target_const,
                target: target_type,
            }
        );
    }

    #[test]
    fn substitutions_apply_constant_arguments_through_callable_conditions_and_dependencies() {
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

        let invocation = store
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::direct(
                    source_subject.clone(),
                    DependencyRequirementKind::StorageAlive,
                ),
            ]))
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

        use crate::{
            CallableConditionSet, CallableConditions, CallableContractClause,
            CallableContractClauseKind, CallableExecutionGuarantee, ExecutionProperty,
            PredicateSemanticSummary,
        };

        let conditions = |dependency, condition| {
            CallableConditionSet::new([
                CallableContractClause::new(
                    SymbolOrdinal::new(0),
                    CallableContractClauseKind::Guard,
                    PredicateSemanticSummary::new(dependency).with_condition(Some(condition)),
                ),
                CallableContractClause::new(
                    SymbolOrdinal::new(1),
                    CallableContractClauseKind::Ensures,
                    PredicateSemanticSummary::new(dependency).with_condition(Some(condition)),
                )
                .with_guard(Some(SymbolOrdinal::new(0))),
            ])
            .with_execution_guarantees([
                CallableExecutionGuarantee::new(
                    ExecutionProperty::Pure,
                    Some(SymbolOrdinal::new(0)),
                ),
                CallableExecutionGuarantee::new(
                    ExecutionProperty::Total,
                    Some(SymbolOrdinal::new(0)),
                ),
            ])
        };

        let subject = store
            .intern_type(TypeData::Callable(
                CallableTypeData::new(
                    [],
                    result,
                    CallableConstness::Runtime,
                    CallableTrust::Safe,
                    CallableAbi::Bray,
                    CallableDependencyContracts::asynchronous(invocation, deferred),
                )
                .with_conditions(conditions(invocation, source_term)),
            ))
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

        let substituted = store
            .type_data(substituted)
            .unwrap_or_else(|error| panic!("substituted type must be available: {error:?}"));

        let TypeData::Callable(callable) = substituted.as_ref() else {
            panic!("substituted type must remain callable");
        };

        let contracts = callable.dependency_contracts();

        assert_eq!(
            callable.conditions(),
            &conditions(contracts.invocation(), target_term)
        );

        let invocation = store
            .dependency_contract_template_data(contracts.invocation())
            .unwrap_or_else(|error| panic!("substituted invocation must be available: {error:?}"));

        let deferred = contracts
            .deferred_execution()
            .unwrap_or_else(|| panic!("substituted callable must remain asynchronous"));

        let deferred = store
            .dependency_contract_template_data(deferred)
            .unwrap_or_else(|error| panic!("substituted deferred contract missing: {error:?}"));

        assert_eq!(
            invocation.as_ref(),
            &DependencyContractTemplateData::new([DependencyRequirement::direct(
                target_subject.clone(),
                DependencyRequirementKind::StorageAlive,
            )])
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
