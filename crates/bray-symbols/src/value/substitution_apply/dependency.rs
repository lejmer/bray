use super::super::DependencyStorageEffect;
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

        Ok(phase_behaviors.with_execution_domains(
            body.execution_domains()
                .iter()
                .map(|domain| {
                    domain.try_map(|predicate| {
                        self.substitute_predicate_data(predicate, substitution)
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ))
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

        let effect = |effect: &DependencyStorageEffect| effect.try_map(
            &mut (),
            |_, subject| self.substitute_dependency_subject(subject, substitution),
            |_, projections| projections.iter()
                .map(|projection| self.substitute_dependency_projection(projection, substitution)).collect(),
            |_, ty| self.substitute_type_data(ty, substitution),
        );

        let effects = template.effects().iter().map(effect).collect::<Result<Vec<_>, _>>()?;

        let captured_values = substituted.captured_values().iter().map(|outcome| {
                    let guard = outcome.guard.map(|term| self.substitute_constant_term_data(term, substitution)).transpose()?;

                    let bindings = outcome.bindings.iter().map(|(destination, value)| {
                        Ok((self.substitute_dependency_subject(destination, substitution)?, value.try_map(
                            &mut |term| self.substitute_constant_term_data(*term, substitution),
                            &mut |projection| self.substitute_dependency_projection(projection, substitution),
                            &mut |root| self.substitute_static_reference(root, substitution), &mut |callee| Ok(super::super::CapturedCallable {
                                instance: self.substitute_callable_instance_data(callee.instance, substitution)?,
                                signature: self.substitute_type_data(callee.signature, substitution)?,
                                receiver: callee.receiver.map(|ty| self.substitute_type_data(ty, substitution)).transpose()?,
                                requirement: callee.requirement.map(|r| Ok(crate::ImplementationRequirementKey::new(
                                    self.substitute_type_data(r.subject(), substitution)?, self.substitute_trait_application_data(r.trait_application(), substitution)?))).transpose()?,
                                dispatch: callee.dispatch.map(|w| self.substitute_implementation_instance_data(w, substitution)).transpose()?,
                                witnesses: callee.witnesses.iter().map(|w| self.substitute_implementation_instance_data(*w, substitution)).collect::<Result<_, _>>()?,
                            }), &mut |variant| Ok(*variant))?))
                    }).collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

                    let ownership = outcome.ownership.try_map(
                    &mut |transfer| {
                        Ok::<_, SemanticValueStoreError>(
                            super::super::DependencyOwnedResult::Transfer(
                                super::super::DependencyOwnedTransfer::new(
                                    self.substitute_constant_term_data(
                                        transfer.input(),
                                        substitution,
                                    )?,
                                    transfer.output().iter().copied(),
                                )
                                .with_preservation(transfer.preservation().clone())
                                .with_condition(
                                    transfer
                                        .condition()
                                        .map(|condition| {
                                            self.substitute_constant_term_data(
                                                condition,
                                                substitution,
                                            )
                                        })
                                        .transpose()?,
                                ),
                            ),
                        )
                    },
                    &mut |condition| {
                        self.substitute_constant_term_data(*condition, substitution)
                            .map(Some)
                    },
                )?;

        let carriers = outcome.carriers
            .iter()
            .map(|carrier| {
                let support = carrier.support().map(|support| support.try_map(
                    &mut (),
                    |_, owner| self.substitute_dependency_subject(owner, substitution),
                    |_, address| self.substitute_constant_term_data(*address, substitution),
                )).transpose()?;

                Ok(carrier.remap(
                    self.substitute_constant_term_data(carrier.guarantee(), substitution)?,
                    carrier
                        .path()
                        .iter()
                        .map(|projection| {
                            self.substitute_dependency_projection(projection, substitution)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ).with_support(support))
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

                    let effects = outcome.effects.iter().map(effect).collect::<Result<Vec<_>, _>>()?;

                    Ok(super::super::CapturedValueOutcome::new(guard, bindings).with_effects(effects).with_carriers(carriers).with_ownership(ownership).with_report(outcome.report.clone())
                        .with_storage(self.substitute_dependency_borrows(&outcome.references, substitution)?,
                            self.substitute_stored_dependencies(&outcome.stored_values, substitution)?)
                        .with_inactive_payloads(outcome.inactive_payloads.iter().map(|payload|
                            self.substitute_dependency_subject(payload, substitution))
                            .collect::<Result<Vec<_>, _>>()?))
                }).collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

        let failures = substituted.failure_outcomes().map(|rows| rows.iter().map(|row| {
            Ok(super::super::CapturedFailureOutcome {
                kind: row.kind,
                guard: row.guard.map(|term| self.substitute_constant_term_data(term, substitution)).transpose()?,
                effects: row.effects.as_deref().map(|effects| effects.iter().map(effect).collect::<Result<_, _>>()).transpose()?,
                report: row.report.clone(),
            })
        }).collect::<Result<Vec<_>, SemanticValueStoreError>>()).transpose()?;

        self.intern_dependency_contract_template(
            substituted.with_captured_values(captured_values).with_failure_outcomes(failures).with_observed_result(
                    template
                        .observed_result()
                        .map(|term| self.substitute_constant_term_data(term, substitution))
                        .transpose()?,
                )
                .with_possible_effects(effects),
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
            DependencyRequirement::CleanupDependencies { ty, requirements } => {
                Ok(DependencyRequirement::cleanup_dependencies(
                    self.substitute_type_data(*ty, substitution)?,
                    requirements
                        .iter()
                        .map(|nested| self.substitute_dependency_requirement(nested, substitution))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            }
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

    fn substitute_dependency_borrows(
        &self,
        references: &[super::super::DependencyBorrowRelation],
        substitution: &Substitution<'_>,
    ) -> Result<Vec<super::super::DependencyBorrowRelation>, SemanticValueStoreError> {
        references
            .iter()
            .map(|relation| {
                Ok(relation.remap(
                    relation
                        .source_subject()
                        .map(|entry| self.substitute_dependency_subject(entry, substitution))
                        .transpose()?,
                    self.substitute_dependency_subject(relation.exit(), substitution)?,
                    relation
                        .target_projections()
                        .iter()
                        .map(|projection| {
                            self.substitute_dependency_projection(projection, substitution)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            })
            .collect::<Result<Vec<_>, SemanticValueStoreError>>()
    }

    fn substitute_stored_dependencies(
        &self,
        values: &[super::super::DependencyStoredValue],
        substitution: &Substitution<'_>,
    ) -> Result<Vec<super::super::DependencyStoredValue>, SemanticValueStoreError> {
        values.iter().map(|stored| {
                    Ok(super::super::DependencyStoredValue::new(
                        super::super::DependencyStoredValuePath::new(self.substitute_dependency_subject(stored.destination().subject(), substitution)?, stored.destination().dereferences()),
                        stored.sources().iter().map(|source| Ok(super::super::DependencyStoredValuePath::new(self.substitute_dependency_subject(source.subject(), substitution)?, source.dereferences()))).collect::<Result<Vec<_>, SemanticValueStoreError>>()?,
                        stored.is_complete(),
                    ))
                }).collect::<Result<Vec<_>, SemanticValueStoreError>>()
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
            DependencySubjectRoot::StaticInstance(reference, duration) => {
                let selection = self.static_reference(reference);
                let selected = self.substitute_static_reference(&selection, substitution)?;

                DependencySubjectRoot::StaticInstance(self.intern_static_reference(selected)?, duration)
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
        let store =
            SemanticValueStore::try_new().expect("fixture semantic value store must be valid");

        let source_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let target_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let formal = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(4));

        let source = store
            .intern_type(TypeData::TypeParameter(source_parameter))
            .expect("fixture type must be interned");

        let target = store
            .intern_type(TypeData::TypeParameter(target_parameter))
            .expect("fixture type must be interned");

        let definition = CallableDefinitionId::try_new(
            FunctionSymbolId::from_symbol_id(SymbolId::new(1)).into(),
        )
        .expect("fixture callable definition id must be valid");

        let owner = GenericOwnerId::try_new(definition.symbol())
            .expect("fixture generic owner id must be valid");

        let argument = store
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .expect("fixture constant term must be interned");

        let other = store
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .expect("fixture constant term must be interned");

        let condition = |ty, argument| {
            let substitution = store
                .intern_generic_substitution(
                    GenericSubstitutionData::try_new(
                        owner,
                        [GenericParameterSymbolId::Type(formal)],
                        [GenericArgument::Type(ty)],
                    )
                    .expect("fixture generic substitution data must be valid"),
                )
                .expect("fixture generic substitution must be interned");

            let callable = store
                .intern_callable_instance(CallableInstanceData::new(definition, substitution))
                .expect("fixture callable instance must be interned");

            store
                .intern_constant_term(ConstantTermData::call(callable, None, [], [argument]))
                .expect("fixture constant term must be interned")
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
            .expect("fixture dependency contract must be interned");

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
                    .expect("fixture `replace_predicate_types` call must succeed"),
                expected
            );

            assert_ne!(
                store
                    .replace_predicate_types(original, &replacements)
                    .expect("fixture `replace_predicate_types` call must succeed"),
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
                store
                    .replace_phase_types(&phases, &replacements)
                    .expect("fixture `replace_phase_types` call must succeed"),
                expected_phases
            );
        }
    }
}
