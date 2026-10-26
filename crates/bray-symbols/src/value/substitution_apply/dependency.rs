use super::context::Substitution;

use super::super::{
    CallableDependencyContracts, ConstantTermData, DependencyGuard, DependencyProjection,
    DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
    GenericSubstitutionData, GenericSubstitutionId, SemanticValueStore, SemanticValueStoreError,
    TypeData,
};
use crate::{GenericOwnerId, GenericParameterSymbolId};

impl SemanticValueStore {
    /// Rewrites type identities throughout a constant term without evaluating it.
    pub fn replace_term_types(
        &self,
        term: crate::ConstantTermId,
        types: &std::collections::BTreeMap<crate::TypeId, crate::TypeId>,
    ) -> Result<crate::ConstantTermId, SemanticValueStoreError> {
        self.substitute_constant_term_data(term, &Substitution::Types(types))
    }

    /// Rewrites predicate type identities without changing its domain or value subjects.
    pub fn replace_predicate_types(
        &self,
        predicate: crate::PredicateSemanticSummary,
        types: &std::collections::BTreeMap<crate::TypeId, crate::TypeId>,
    ) -> Result<crate::PredicateSemanticSummary, SemanticValueStoreError> {
        self.substitute_predicate_data(predicate, &Substitution::Types(types))
    }

    /// Rewrites types throughout phases using the same traversal as generic instantiation.
    pub fn replace_phase_types(
        &self,
        phases: &crate::CallablePhaseBehaviors,
        types: &std::collections::BTreeMap<crate::TypeId, crate::TypeId>,
    ) -> Result<crate::CallablePhaseBehaviors, SemanticValueStoreError> {
        self.substitute_callable_phase_behaviors(phases, &Substitution::Types(types))
    }

    pub(super) fn substitute_predicate_data(
        &self,
        predicate: crate::PredicateSemanticSummary,
        substitution: &Substitution<'_>,
    ) -> Result<crate::PredicateSemanticSummary, SemanticValueStoreError> {
        Ok(crate::PredicateSemanticSummary::new(
            self.substitute_dependency_contract_data(
                predicate.dependency_contract(),
                substitution,
            )?,
        )
        .with_condition(
            predicate
                .condition()
                .map(|term| self.substitute_constant_term_data(term, substitution))
                .transpose()?,
            predicate.domain(),
        ))
    }

    pub(super) fn substitute_callable_phase_behaviors(
        &self,
        phases: &crate::CallablePhaseBehaviors,
        substitution: &Substitution<'_>,
    ) -> Result<crate::CallablePhaseBehaviors, SemanticValueStoreError> {
        let dependencies = self.substitute_callable_dependency_contracts(
            phases.dependency_contracts(),
            substitution,
        )?;

        // Phase behavior is immutable and Arc-backed. Preserve unchanged execution properties.
        let phase_behaviors = phases
            .clone()
            .try_with_dependency_contracts(dependencies)
            .expect("substitution preserves callable phase contracts");

        let body = phases
            .deferred_execution()
            .unwrap_or_else(|| phases.invocation());

        let phase_behaviors = phase_behaviors.with_receiver_cleanup(
            body.receiver_cleanup()
                .iter()
                .map(|ty| self.substitute_type_data(*ty, substitution))
                .collect::<Result<Vec<_>, _>>()?,
        );

        let phase_behaviors = phase_behaviors.with_predicates(
            phases
                .invocation()
                .predicate_requirements()
                .iter()
                .copied()
                .map(|predicate| self.substitute_predicate_data(predicate, substitution))
                .collect::<Result<Vec<_>, _>>()?,
            body.predicate_guarantees()
                .iter()
                .copied()
                .map(|predicate| self.substitute_predicate_data(predicate, substitution))
                .collect::<Result<Vec<_>, _>>()?,
        );

        Ok(phase_behaviors)
    }

    /// Applies one generic substitution throughout a dependency contract.
    pub fn substitute_dependency_contract(
        &self,
        contract: super::super::DependencyContractTemplateId,
        substitution: GenericSubstitutionId,
    ) -> Result<super::super::DependencyContractTemplateId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(substitution);

        self.substitute_dependency_contract_data(contract, &Substitution::Generic(&substitution))
    }

    /// Replaces a declaration's `Self` throughout its returned-value and cleanup contract.
    pub fn substitute_contextual_self_in_dependency_contract(
        &self,
        contract: super::super::DependencyContractTemplateId,
        context: crate::SelfTypeContext,
        replacement: crate::TypeId,
    ) -> Result<super::super::DependencyContractTemplateId, SemanticValueStoreError> {
        let contextual = self.intern_type(TypeData::ContextualSelf(context))?;
        let types = std::collections::BTreeMap::from([(contextual, replacement)]);

        self.substitute_dependency_contract_data(contract, &Substitution::Types(&types))
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
        substitution: &Substitution<'_>,
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
        substitution: &Substitution<'_>,
    ) -> Result<super::super::DependencyContractTemplateId, SemanticValueStoreError> {
        let template = self.dependency_contract_template_data(template);

        let substituted = template.try_map_requirements(|_, requirements| {
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
        substitution: &Substitution<'_>,
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
        substitution: &Substitution<'_>,
    ) -> Result<DependencyRequirement, SemanticValueStoreError> {
        match requirement {
            DependencyRequirement::SynchronousCleanup(ty) => {
                Ok(DependencyRequirement::SynchronousCleanup(
                    self.substitute_type_data(*ty, substitution)?,
                ))
            }
            DependencyRequirement::FinalizerBodyCleanup(ty) => {
                Ok(DependencyRequirement::FinalizerBodyCleanup(
                    self.substitute_type_data(*ty, substitution)?,
                ))
            }
            DependencyRequirement::DefaultCleanup {
                provider,
                substitution: selected,
                contextual_self,
            } => Ok(DependencyRequirement::DefaultCleanup {
                provider: *provider,
                substitution: self.substitute_generic_substitution_data(*selected, substitution)?,
                contextual_self: contextual_self
                    .map(|(context, subject)| {
                        self.substitute_type_data(subject, substitution)
                            .map(|subject| (context, subject))
                    })
                    .transpose()?,
            }),
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
        substitution: &Substitution<'_>,
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
        substitution: &Substitution<'_>,
    ) -> Result<DependencySubject, SemanticValueStoreError> {
        let root = match subject.subject_root() {
            DependencySubjectRoot::ImplementationWitness(instance) => {
                DependencySubjectRoot::ImplementationWitness(
                    self.substitute_implementation_instance_data(instance, substitution)?,
                )
            }
            DependencySubjectRoot::ImplementationRequirement(requirement) => {
                DependencySubjectRoot::ImplementationRequirement(
                    crate::ImplementationRequirementKey::new(
                        self.substitute_type_data(requirement.subject(), substitution)?,
                        self.substitute_trait_application_data(
                            requirement.trait_application(),
                            substitution,
                        )?,
                    ),
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
        substitution: &Substitution<'_>,
    ) -> Result<DependencyProjection, SemanticValueStoreError> {
        match projection {
            DependencyProjection::Element(index) => self
                .substitute_constant_term_data(*index, substitution)
                .map(DependencyProjection::Element),
            projection => Ok(*projection),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        CallableDefinitionId, CallableInstanceData, CallablePhaseBehavior, CallablePhaseBehaviors,
        ConstantTermData, DependencyContractTemplateData, DependencyRequirement,
        DependencyRequirementKind, DependencySubject, DependencySubjectRoot, FunctionSymbolId,
        GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        GenericTypeParameterSymbolId, PredicateDomain, PredicateSemanticSummary,
        SemanticValueStore, SymbolId, SymbolOrdinal, TypeData,
    };

    #[test]
    fn type_replacement_preserves_predicate_domains_and_value_subjects() {
        let store = SemanticValueStore::try_new().unwrap();
        let source_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let target_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let formal = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(4));

        let source = store
            .intern_type(TypeData::TypeParameter(source_parameter))
            .unwrap();

        let target = store
            .intern_type(TypeData::TypeParameter(target_parameter))
            .unwrap();

        let definition = CallableDefinitionId::try_new(
            FunctionSymbolId::from_symbol_id(SymbolId::new(1)).into(),
        )
        .unwrap();

        let owner = GenericOwnerId::try_new(definition.symbol()).unwrap();

        let argument = store
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let other = store
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let condition = |ty, argument| {
            let substitution = store
                .intern_generic_substitution(
                    GenericSubstitutionData::try_new(
                        owner,
                        [GenericParameterSymbolId::Type(formal)],
                        [GenericArgument::Type(ty)],
                    )
                    .unwrap(),
                )
                .unwrap();

            let callable = store
                .intern_callable_instance(CallableInstanceData::new(definition, substitution))
                .unwrap();

            store
                .intern_constant_term(ConstantTermData::call(callable, None, [argument]))
                .unwrap()
        };

        let dependency = store
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(
                        0,
                    ))),
                    DependencyRequirementKind::StorageAlive,
                ),
            ]))
            .unwrap();

        let replacements = BTreeMap::from([(source, target)]);

        for domain in [
            PredicateDomain::Value,
            PredicateDomain::Trusted,
            PredicateDomain::Execution,
        ] {
            let original = PredicateSemanticSummary::new(dependency)
                .with_condition(Some(condition(source, argument)), domain);

            let expected = PredicateSemanticSummary::new(dependency)
                .with_condition(Some(condition(target, argument)), domain);

            let different = PredicateSemanticSummary::new(dependency)
                .with_condition(Some(condition(target, other)), domain);

            assert_eq!(
                store
                    .replace_predicate_types(original, &replacements)
                    .unwrap(),
                expected
            );

            assert_ne!(
                store
                    .replace_predicate_types(original, &replacements)
                    .unwrap(),
                different
            );

            let phases = CallablePhaseBehaviors::asynchronous(
                CallablePhaseBehavior::empty(dependency),
                CallablePhaseBehavior::empty(dependency),
            )
            .with_predicates([original], [original])
            .with_receiver_cleanup([source]);

            let expected_phases = phases
                .clone()
                .with_predicates([expected], [expected])
                .with_receiver_cleanup([target]);

            assert_eq!(
                store.replace_phase_types(&phases, &replacements).unwrap(),
                expected_phases
            );
        }
    }
}
