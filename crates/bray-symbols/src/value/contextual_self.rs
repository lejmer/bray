use super::{
    CallableParameterData, CallableTypeData, GenericArgument, GenericSubstitutionData,
    GenericSubstitutionId, SelfTypeContext, SemanticValueStore, SemanticValueStoreError,
    TraitApplicationData, TraitApplicationId, TypeData, TypeId,
};

impl SemanticValueStore {
    /// Replaces one declaration context's `Self` throughout a semantic type.
    pub fn substitute_contextual_self(
        &self,
        ty: TypeId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let data = self.type_data(ty)?;

        if matches!(data.as_ref(), TypeData::ContextualSelf(candidate) if *candidate == context) {
            return Ok(replacement);
        }

        let substituted = match data.as_ref() {
            TypeData::Error | TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => {
                return Ok(ty);
            }
            TypeData::Named {
                definition,
                substitution,
            } => TypeData::Named {
                definition: *definition,
                substitution: self.substitute_contextual_self_in_substitution(
                    *substitution,
                    context,
                    replacement,
                )?,
            },
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => TypeData::TypeValuedMemberProjection {
                subject: self.substitute_contextual_self(*subject, context, replacement)?,
                application: self.substitute_contextual_self_in_application(
                    *application,
                    context,
                    replacement,
                )?,
                member: *member,
            },
            TypeData::Tuple(elements) => TypeData::tuple(
                elements
                    .iter()
                    .map(|element| self.substitute_contextual_self(*element, context, replacement))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TypeData::Array { element, length } => TypeData::Array {
                element: self.substitute_contextual_self(*element, context, replacement)?,
                length: *length,
            },
            TypeData::FlexibleArray(element) => TypeData::FlexibleArray(
                self.substitute_contextual_self(*element, context, replacement)?,
            ),
            TypeData::Slice(element) => {
                TypeData::Slice(self.substitute_contextual_self(*element, context, replacement)?)
            }
            TypeData::Generator(element) => TypeData::Generator(self.substitute_contextual_self(
                *element,
                context,
                replacement,
            )?),
            TypeData::Nullable(target) => TypeData::Nullable(self.substitute_contextual_self(
                *target,
                context,
                replacement,
            )?),
            TypeData::Borrow { kind, target } => TypeData::Borrow {
                kind: *kind,
                target: self.substitute_contextual_self(*target, context, replacement)?,
            },
            TypeData::TraitView(application) => TypeData::TraitView(
                self.substitute_contextual_self_in_application(*application, context, replacement)?,
            ),
            TypeData::OwnedIndirection { storage, target } => TypeData::OwnedIndirection {
                storage: self.substitute_contextual_self(*storage, context, replacement)?,
                target: self.substitute_contextual_self(*target, context, replacement)?,
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
                            self.substitute_contextual_self(parameter.ty(), context, replacement)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

                TypeData::Callable(
                    CallableTypeData::new(
                        parameters,
                        self.substitute_contextual_self(callable.result(), context, replacement)?,
                        callable.constness(),
                        callable.trust(),
                        callable.abi(),
                        callable.dependency_contracts(),
                    )
                    .with_variadic(callable.is_variadic())
                    .with_phase_behaviors(callable.phase_behaviors().clone()),
                )
            }
        };

        self.intern_type(substituted)
    }

    /// Replaces one declaration context's `Self` throughout a trait application.
    pub fn substitute_contextual_self_in_application(
        &self,
        application: TraitApplicationId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<TraitApplicationId, SemanticValueStoreError> {
        let data = self.trait_application_data(application)?;

        let substitution = self.substitute_contextual_self_in_substitution(
            data.substitution(),
            context,
            replacement,
        )?;

        self.intern_trait_application(TraitApplicationData::new(data.definition(), substitution))
    }

    /// Replaces a declaration's `Self` in symbolic returned-value dependencies.
    pub fn substitute_contextual_self_in_dependency_contract(
        &self,
        contract: super::DependencyContractTemplateId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<super::DependencyContractTemplateId, SemanticValueStoreError> {
        let template = self.dependency_contract_template_data(contract)?;

        let requirements = self.substitute_contextual_dependency_requirements(
            template.requirements(),
            context,
            replacement,
        )?;

        self.intern_dependency_contract_template(super::DependencyContractTemplateData::new(
            requirements,
        ))
    }

    fn substitute_contextual_call_inputs(
        &self,
        inputs: &[super::DependencyCallInput],
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<Vec<super::DependencyCallInput>, SemanticValueStoreError> {
        inputs
            .iter()
            .map(|input| {
                Ok(super::DependencyCallInput::new(
                    input.root(),
                    self.substitute_contextual_dependency_requirements(
                        input.values(),
                        context,
                        replacement,
                    )?,
                    self.substitute_contextual_dependency_requirements(
                        input.storage(),
                        context,
                        replacement,
                    )?,
                ))
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()
    }

    fn substitute_contextual_dependency_requirements(
        &self,
        requirements: &[super::DependencyRequirement],
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<Vec<super::DependencyRequirement>, SemanticValueStoreError> {
        // Unchanged direct subjects and guards retain their immutable shared snapshots.
        requirements
            .iter()
            .map(|item| match item {
                super::DependencyRequirement::FixedPoint {
                    definitions,
                    result,
                } => {
                    let definitions = definitions
                        .iter()
                        .map(|definition| {
                            self.substitute_contextual_dependency_requirements(
                                definition,
                                context,
                                replacement,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    let result = self.substitute_contextual_dependency_requirements(
                        result,
                        context,
                        replacement,
                    )?;

                    Ok(super::DependencyRequirement::fixed_point(
                        definitions,
                        result,
                    ))
                }
                super::DependencyRequirement::Variable { depth, ordinal } => {
                    Ok(super::DependencyRequirement::variable(*depth, *ordinal))
                }
                super::DependencyRequirement::RecursiveCall { callable, inputs } => {
                    let callable = self.callable_instance_data(*callable)?;

                    let substitution = self.substitute_contextual_self_in_substitution(
                        callable.substitution(),
                        context,
                        replacement,
                    )?;

                    let callable = self.intern_callable_instance(
                        super::CallableInstanceData::new(callable.definition(), substitution),
                    )?;

                    let inputs =
                        self.substitute_contextual_call_inputs(inputs, context, replacement)?;

                    Ok(super::DependencyRequirement::recursive_call(
                        callable, inputs,
                    ))
                }
                super::DependencyRequirement::WitnessCall {
                    callable,
                    requirement,
                    inputs,
                } => {
                    let callable = self.callable_instance_data(*callable)?;

                    let substitution = self.substitute_contextual_self_in_substitution(
                        callable.substitution(),
                        context,
                        replacement,
                    )?;

                    let callable = self.intern_callable_instance(
                        super::CallableInstanceData::new(callable.definition(), substitution),
                    )?;

                    let subject = self.substitute_contextual_self(
                        requirement.subject(),
                        context,
                        replacement,
                    )?;

                    let application = self.substitute_contextual_self_in_application(
                        requirement.trait_application(),
                        context,
                        replacement,
                    )?;

                    let inputs =
                        self.substitute_contextual_call_inputs(inputs, context, replacement)?;

                    Ok(super::DependencyRequirement::witness_call(
                        callable,
                        crate::ImplementationRequirementKey::new(subject, application),
                        inputs,
                    ))
                }
                super::DependencyRequirement::Guarded(guarded) => {
                    Ok(super::DependencyRequirement::guarded(
                        guarded.guard().clone(),
                        self.substitute_contextual_dependency_requirements(
                            guarded.requirements(),
                            context,
                            replacement,
                        )?,
                    ))
                }
                super::DependencyRequirement::Direct { .. } => Ok(item.clone()),
            })
            .collect()
    }

    /// Replaces one declaration context's `Self` throughout a generic substitution.
    pub fn substitute_contextual_self_in_substitution(
        &self,
        substitution: GenericSubstitutionId,
        context: SelfTypeContext,
        replacement: TypeId,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let data = self.generic_substitution_data(substitution)?;

        let arguments = data
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => self
                    .substitute_contextual_self(ty, context, replacement)
                    .map(GenericArgument::Type),
                GenericArgument::Constant(term) => Ok(GenericArgument::Constant(term)),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let substituted = GenericSubstitutionData::try_new(
            data.owner(),
            data.bindings().iter().map(|binding| binding.parameter()),
            arguments,
        )
        .map_err(|_| SemanticValueStoreError::OpenSubstitution)?;

        self.intern_generic_substitution(substituted)
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticValueStore;
    use crate::{
        SelfTypeContext, SemanticValueStoreError, StructSymbolId, SymbolId, TraitSymbolId, TypeData,
    };

    #[test]
    fn contextual_self_substitution_reaches_nested_type_layers() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let context = SelfTypeContext::Trait(TraitSymbolId::from_symbol_id(SymbolId::new(1)));

        let replacement = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("replacement type interning failed: {error:?}"));

        let contextual = store
            .intern_type(TypeData::ContextualSelf(context))
            .unwrap_or_else(|error| panic!("contextual type interning failed: {error:?}"));

        let subject = store
            .intern_type(TypeData::Nullable(contextual))
            .unwrap_or_else(|error| panic!("subject type interning failed: {error:?}"));

        let substituted = store
            .substitute_contextual_self(subject, context, replacement)
            .unwrap_or_else(|error| panic!("contextual substitution failed: {error:?}"));

        let data = store
            .type_data(substituted)
            .unwrap_or_else(|error| panic!("substituted type missing: {error:?}"));

        assert_eq!(data.as_ref(), &TypeData::Nullable(replacement));
    }

    #[test]
    fn contextual_self_substitution_preserves_other_contexts() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let source = SelfTypeContext::Trait(TraitSymbolId::from_symbol_id(SymbolId::new(1)));

        let other = SelfTypeContext::NamedType(crate::NamedTypeSymbolId::Struct(
            StructSymbolId::from_symbol_id(SymbolId::new(2)),
        ));

        let subject = store
            .intern_type(TypeData::ContextualSelf(other))
            .unwrap_or_else(|error| panic!("subject type interning failed: {error:?}"));

        let replacement = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("replacement type interning failed: {error:?}"));

        let result: Result<_, SemanticValueStoreError> =
            store.substitute_contextual_self(subject, source, replacement);

        assert_eq!(result, Ok(subject));
    }
}
