use super::super::{
    CallableDependencyContracts, ConstantTermData, DependencyGuard, DependencyProjection,
    DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
    GenericSubstitutionData, GenericSubstitutionId, SemanticValueStore, SemanticValueStoreError,
    TypeData,
};
use crate::{GenericOwnerId, GenericParameterSymbolId};

impl SemanticValueStore {
    /// Applies one generic substitution throughout a dependency contract.
    pub fn substitute_dependency_contract(
        &self,
        contract: super::super::DependencyContractTemplateId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::super::DependencyContractTemplateId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_dependency_contract_data(contract, &substitution)
    }

    /// Reports whether a generic parameter occurs anywhere in a dependency contract.
    pub fn dependency_contract_uses_parameter(
        &self,
        template: super::super::DependencyContractTemplateId,
        owner: GenericOwnerId,
        parameter: GenericParameterSymbolId,
    ) -> Result<bool, SemanticValueStoreError> {
        // Substitution is structural and does not evaluate terms. Replacing one parameter with a
        // closed value detects every occurrence using the same traversal as contract instantiation.
        let replacement = match parameter {
            GenericParameterSymbolId::Type(_) => {
                GenericArgument::Type(self.intern_type(TypeData::tuple([]))?)
            }
            GenericParameterSymbolId::Const(_) => {
                GenericArgument::Constant(self.intern_constant_term(ConstantTermData::tuple([]))?)
            }
        };

        let substitution = GenericSubstitutionData::try_new(owner, [parameter], [replacement])
            .expect("substitution preserves parameter kinds and arity");

        let substitution = self.intern_generic_substitution(substitution)?;

        Ok(self.substitute_dependency_contract(template, substitution)? != template)
    }

    pub(super) fn substitute_callable_dependency_contracts(
        &self,
        contracts: CallableDependencyContracts,
        substitution: &GenericSubstitutionData,
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

    pub(super) fn substitute_dependency_contract_data(
        &self,
        template: super::super::DependencyContractTemplateId,
        substitution: &GenericSubstitutionData,
    ) -> Result<super::super::DependencyContractTemplateId, SemanticValueStoreError> {
        let template = self.dependency_contract_template_data(template);

        let substituted = template.try_map_requirements(|requirements| {
            requirements
                .iter()
                .map(|requirement| {
                    self.substitute_dependency_requirement(requirement, substitution)
                })
                .collect::<Result<Vec<_>, _>>()
        })?;

        let borrows = template
            .borrows()
            .iter()
            .map(|relation| {
                Ok(super::super::DependencyBorrowRelation::new(
                    relation
                        .entry()
                        .map(|entry| self.substitute_dependency_subject(entry, substitution))
                        .transpose()?,
                    self.substitute_dependency_subject(relation.exit(), substitution)?,
                    relation.kind(),
                )
                .with_dereferences(relation.dereferences())
                .with_exactness(relation.is_exact())
                .with_target_projections(
                    relation
                        .target_projections()
                        .iter()
                        .map(|projection| {
                            self.substitute_dependency_projection(projection, substitution)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

        let carriers = template
            .carriers()
            .iter()
            .map(|carrier| {
                Ok(super::super::DependencyWitnessCarrier::new(
                    self.substitute_constant_term_data(carrier.guarantee(), substitution)?,
                    carrier
                        .path()
                        .iter()
                        .map(|projection| {
                            self.substitute_dependency_projection(projection, substitution)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

        let effects = template
            .effects()
            .iter()
            .map(|effect| {
                use super::super::DependencyStorageEffect;

                Ok(match effect {
                    DependencyStorageEffect::Atomic => DependencyStorageEffect::Atomic,
                    DependencyStorageEffect::Ambient => DependencyStorageEffect::Ambient,
                    DependencyStorageEffect::Unknown => DependencyStorageEffect::Unknown,
                    DependencyStorageEffect::Escape => DependencyStorageEffect::Escape,
                    DependencyStorageEffect::Cleanup(ty) => DependencyStorageEffect::Cleanup(
                        self.substitute_type_data(*ty, substitution)?,
                    ),
                    DependencyStorageEffect::Direct { subject, kind } => {
                        DependencyStorageEffect::Direct {
                            subject: self.substitute_dependency_subject(subject, substitution)?,
                            kind: kind
                                .try_map_type(|ty| self.substitute_type_data(ty, substitution))?,
                        }
                    }
                    DependencyStorageEffect::Indirect {
                        reference,
                        projections,
                        kind,
                    } => DependencyStorageEffect::Indirect {
                        reference: self.substitute_dependency_subject(reference, substitution)?,
                        projections: projections
                            .iter()
                            .map(|projection| {
                                self.substitute_dependency_projection(projection, substitution)
                            })
                            .collect::<Result<_, SemanticValueStoreError>>()?,
                        kind: kind
                            .try_map_type(|ty| self.substitute_type_data(ty, substitution))?,
                    },
                })
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

        self.intern_dependency_contract_template(
            substituted
                .with_borrows(borrows)
                .with_carriers(carriers)
                .with_observed_result(
                    template
                        .observed_result()
                        .map(|term| self.substitute_constant_term_data(term, substitution))
                        .transpose()?,
                )
                .with_returned_input(
                    template
                        .returned_input()
                        .map(|input| self.substitute_constant_term_data(input, substitution))
                        .transpose()?,
                )
                .with_effects(effects),
        )
    }

    fn substitute_dependency_call_inputs(
        &self,
        inputs: &[super::super::DependencyCallInput],
        substitution: &GenericSubstitutionData,
    ) -> Result<Vec<super::super::DependencyCallInput>, SemanticValueStoreError> {
        inputs
            .iter()
            .map(|input| {
                Ok(super::super::DependencyCallInput::new(
                    input.root(),
                    input
                        .values()
                        .iter()
                        .map(|value| self.substitute_dependency_requirement(value, substitution))
                        .collect::<Result<Vec<_>, SemanticValueStoreError>>()?,
                    input
                        .storage()
                        .iter()
                        .map(|value| self.substitute_dependency_requirement(value, substitution))
                        .collect::<Result<Vec<_>, SemanticValueStoreError>>()?,
                ))
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()
    }

    fn substitute_dependency_requirement(
        &self,
        requirement: &DependencyRequirement,
        substitution: &GenericSubstitutionData,
    ) -> Result<DependencyRequirement, SemanticValueStoreError> {
        match requirement {
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => {
                let definitions = definitions
                    .iter()
                    .map(|definition| {
                        definition
                            .iter()
                            .map(|value| {
                                self.substitute_dependency_requirement(value, substitution)
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

                let result = result
                    .iter()
                    .map(|value| self.substitute_dependency_requirement(value, substitution))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(DependencyRequirement::fixed_point(definitions, result))
            }
            DependencyRequirement::Variable { depth, ordinal } => {
                Ok(DependencyRequirement::variable(*depth, *ordinal))
            }
            DependencyRequirement::ResultCall {
                output,
                callable,
                requirement,
                inputs,
            } => {
                let callable = self.substitute_callable_instance_data(*callable, substitution)?;

                let requirement = requirement
                    .map(|requirement| {
                        let subject =
                            self.substitute_type_data(requirement.subject(), substitution)?;

                        let application = self.substitute_trait_application_data(
                            requirement.trait_application(),
                            substitution,
                        )?;

                        Ok::<_, SemanticValueStoreError>(crate::ImplementationRequirementKey::new(
                            subject,
                            application,
                        ))
                    })
                    .transpose()?;

                let inputs = self.substitute_dependency_call_inputs(inputs, substitution)?;

                Ok(DependencyRequirement::result_call(
                    callable,
                    requirement,
                    inputs,
                    *output,
                ))
            }
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
        substitution: &GenericSubstitutionData,
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
        substitution: &GenericSubstitutionData,
    ) -> Result<DependencySubject, SemanticValueStoreError> {
        let root = match subject.subject_root() {
            DependencySubjectRoot::ImplementationWitness(instance) => {
                DependencySubjectRoot::ImplementationWitness(
                    self.substitute_implementation_instance_data(instance, substitution)?,
                )
            }
            root => root,
        };

        let projections = subject
            .projections()
            .iter()
            .map(|projection| self.substitute_dependency_projection(projection, substitution))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DependencySubject::new(root, projections))
    }

    fn substitute_dependency_projection(
        &self,
        projection: &DependencyProjection,
        substitution: &GenericSubstitutionData,
    ) -> Result<DependencyProjection, SemanticValueStoreError> {
        match projection {
            DependencyProjection::Element(index) => self
                .substitute_constant_term_data(*index, substitution)
                .map(DependencyProjection::Element),
            projection => Ok(*projection),
        }
    }
}
