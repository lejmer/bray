use bray_symbols::{
    DependencyContractTemplateData, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencySubject, DependencySubjectRoot, SemanticValueStore,
};

use crate::{
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceSemantics,
};

use super::common::{convert_requirement_kind, resolve_exact};
use super::{InterfaceSemanticInternError, InterfaceSymbolResolver, InternState};

impl InternState {
    pub(super) fn intern_dependency_contracts(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.dependency_contracts.iter().enumerate() {
            if self.dependency_contracts[index].is_some() {
                continue;
            }

            let Some(requirements) =
                self.convert_dependency_requirements(&input.requirements, symbols)?
            else {
                continue;
            };

            let mut contract = DependencyContractTemplateData::new(requirements);

            if let Some(requirements) = &input.completion_reports {
                let Some(requirements) =
                    self.convert_dependency_requirements(requirements, symbols)?
                else {
                    continue;
                };

                contract = contract.with_completion_reports(requirements);
            }

            if let Some(requirements) = &input.abnormal_reports {
                let Some(requirements) =
                    self.convert_dependency_requirements(requirements, symbols)?
                else {
                    continue;
                };

                contract = contract.with_abnormal_reports(requirements);
            }

            let mut borrows = Vec::with_capacity(input.borrows.len());

            for relation in input.borrows.iter() {
                let (Some(entry), Some(exit)) = (
                    self.convert_dependency_subject(relation.entry(), symbols)?,
                    self.convert_dependency_subject(relation.exit(), symbols)?,
                ) else {
                    break;
                };

                let Some(projections) =
                    self.convert_dependency_projections(relation.target_projections(), symbols)?
                else {
                    break;
                };

                borrows.push(
                    bray_symbols::DependencyBorrowRelation::new(entry, exit, relation.kind())
                        .with_dereferences(relation.dereferences())
                        .with_target_projections(projections),
                );
            }

            if borrows.len() != input.borrows.len() {
                continue;
            }

            let mut carriers = Vec::with_capacity(input.carriers.len());

            for carrier in &*input.carriers {
                let Some(path) = self.convert_dependency_projections(carrier.path(), symbols)?
                else {
                    break;
                };

                let Some(condition) = self.constant_term_id(carrier.guarantee()) else {
                    break;
                };

                carriers.push(bray_symbols::DependencyWitnessCarrier::new(condition, path));
            }

            if carriers.len() != input.carriers.len() {
                continue;
            }

            let Some(effects) = self.convert_dependency_effects(&input.effects, symbols)? else {
                continue;
            };

            let returned_input = if let Some(input) = input.returned_input {
                let Some(input) = self.constant_term_id(input) else {
                    continue;
                };

                Some(input)
            } else {
                None
            };

            let observed_result = if let Some(term) = input.observed_result {
                let Some(term) = self.constant_term_id(term) else { continue; };
                Some(term)
            } else { None };

            self.dependency_contracts[index] = Some(
                store.intern_dependency_contract_template(
                    contract
                        .with_borrows(borrows)
                        .with_carriers(carriers)
                        .with_returned_input(returned_input)
                        .with_observed_result(observed_result)
                        .with_effects(effects)
                        .with_normal_completion(input.normal_completion)
                        .with_normal_storage_preservation(input.normal_storage_preserved),
                )?,
            );
        }

        Ok(())
    }

    pub(super) fn convert_dependency_requirements(
        &self,
        inputs: &[InterfaceDependencyRequirement],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<DependencyRequirement>>, InterfaceSemanticInternError> {
        let mut values = Vec::with_capacity(inputs.len());

        for input in inputs {
            let value = match &input.value {
                InterfaceDependencyRequirementValue::Variable { depth, ordinal } => {
                    DependencyRequirement::variable(*depth, *ordinal)
                }
                InterfaceDependencyRequirementValue::FixedPoint {
                    definitions,
                    result,
                } => {
                    let mut converted = Vec::with_capacity(definitions.len());

                    for definition in definitions.iter() {
                        let Some(value) =
                            self.convert_dependency_requirements(definition, symbols)?
                        else {
                            return Ok(None);
                        };

                        converted.push(value);
                    }

                    let Some(result) = self.convert_dependency_requirements(result, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::fixed_point(converted, result)
                }
                InterfaceDependencyRequirementValue::ResultCall {
                    output,
                    callable,
                    requirement,
                    inputs,
                } => {
                    let Some(callable) = self.callable_instance_id(*callable) else {
                        return Ok(None);
                    };

                    let requirement = match requirement {
                        Some((subject, application)) => {
                            let (Some(subject), Some(application)) = (
                                self.type_id(*subject),
                                self.trait_application_id(*application),
                            ) else {
                                return Ok(None);
                            };

                            Some(bray_symbols::ImplementationRequirementKey::new(
                                subject,
                                application,
                            ))
                        }
                        None => None,
                    };

                    let Some(converted) = self.convert_dependency_call_inputs(inputs, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::result_call(callable, requirement, converted, *output)
                }
                InterfaceDependencyRequirementValue::Direct { subject, kind } => {
                    let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                        return Ok(None);
                    };

                    DependencyRequirement::direct(subject, convert_requirement_kind(*kind))
                }
                InterfaceDependencyRequirementValue::Guarded {
                    guard,
                    requirements,
                } => {
                    let Some(guard) = self.convert_dependency_guard(guard, symbols)? else {
                        return Ok(None);
                    };

                    let Some(requirements) =
                        self.convert_dependency_requirements(requirements, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::guarded(guard, requirements)
                }
            };

            values.push(value);
        }

        Ok(Some(values))
    }

    fn convert_dependency_call_inputs(
        &self,
        inputs: &[crate::InterfaceDependencyCallInput],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<bray_symbols::DependencyCallInput>>, InterfaceSemanticInternError> {
        let mut converted = Vec::new();

        for input in inputs.iter() {
            let Some(root) = self.convert_dependency_subject(
                &InterfaceDependencySubject::new(input.root.clone(), []),
                symbols,
            )?
            else {
                return Ok(None);
            };

            let Some(values) = self.convert_dependency_requirements(&input.values, symbols)? else {
                return Ok(None);
            };

            let Some(storage) = self.convert_dependency_requirements(&input.storage, symbols)?
            else {
                return Ok(None);
            };

            converted.push(bray_symbols::DependencyCallInput::new(
                root.subject_root(),
                values,
                storage,
            ));
        }

        Ok(Some(converted))
    }

    pub(super) fn convert_dependency_subject(
        &self,
        input: &InterfaceDependencySubject,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<DependencySubject>, InterfaceSemanticInternError> {
        let root = match &input.root {
            InterfaceDependencySubjectRoot::Receiver => DependencySubjectRoot::Receiver,
            InterfaceDependencySubjectRoot::Parameter(ordinal) => {
                DependencySubjectRoot::Parameter(*ordinal)
            }
            InterfaceDependencySubjectRoot::Result => DependencySubjectRoot::Result,
            InterfaceDependencySubjectRoot::EvaluationStorage => {
                DependencySubjectRoot::EvaluationStorage
            }
            InterfaceDependencySubjectRoot::ScopedCapability(ordinal) => {
                DependencySubjectRoot::ScopedCapability(*ordinal)
            }
            InterfaceDependencySubjectRoot::ImplementationWitness(id) => {
                let Some(id) = self.implementation_instance_id(*id) else {
                    return Ok(None);
                };

                DependencySubjectRoot::ImplementationWitness(id)
            }
            InterfaceDependencySubjectRoot::ProductStatic(reference) => {
                DependencySubjectRoot::ProductStatic(resolve_exact(symbols, reference)?)
            }
            InterfaceDependencySubjectRoot::ExactThreadStatic(reference) => {
                DependencySubjectRoot::ExactThreadStatic(resolve_exact(symbols, reference)?)
            }
        };

        let Some(projections) = self.convert_dependency_projections(&input.projections, symbols)?
        else {
            return Ok(None);
        };

        Ok(Some(DependencySubject::new(root, projections)))
    }

    fn convert_dependency_projections(
        &self,
        input: &[InterfaceDependencyProjection],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<DependencyProjection>>, InterfaceSemanticInternError> {
        let mut projections = Vec::with_capacity(input.len());

        for projection in input {
            let projection = match projection {
                InterfaceDependencyProjection::ProductField(field) => {
                    DependencyProjection::ProductField(resolve_exact(symbols, field)?)
                }
                InterfaceDependencyProjection::TupleElement(ordinal) => {
                    DependencyProjection::TupleElement(*ordinal)
                }
                InterfaceDependencyProjection::Element(id) => {
                    let Some(id) = self.constant_term_id(*id) else {
                        return Ok(None);
                    };

                    DependencyProjection::Element(id)
                }
                InterfaceDependencyProjection::NullableValue => DependencyProjection::NullableValue,
                InterfaceDependencyProjection::UnionPayloadField(field) => {
                    DependencyProjection::UnionPayloadField(resolve_exact(symbols, field)?)
                }
                InterfaceDependencyProjection::OwnedTarget => DependencyProjection::OwnedTarget,
                InterfaceDependencyProjection::UninitPayload => DependencyProjection::UninitPayload,
            };

            projections.push(projection);
        }

        Ok(Some(projections))
    }

    fn convert_dependency_effects(
        &self,
        input: &[bray_symbols::DependencyStorageEffect<
            InterfaceDependencySubject,
            InterfaceDependencyProjection,
            crate::InterfaceTypeId,
        >],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<bray_symbols::DependencyStorageEffect>>, InterfaceSemanticInternError>
    {
        use bray_symbols::DependencyStorageEffect;

        let mut effects = Vec::with_capacity(input.len());

        for effect in input {
            effects.push(match effect {
                DependencyStorageEffect::Atomic => DependencyStorageEffect::Atomic,
                DependencyStorageEffect::Unknown => DependencyStorageEffect::Unknown,
                DependencyStorageEffect::Escape => DependencyStorageEffect::Escape,
                DependencyStorageEffect::Cleanup(ty) => {
                    let Some(ty) = self.type_id(*ty) else {
                        return Ok(None);
                    };

                    DependencyStorageEffect::Cleanup(ty)
                }
                DependencyStorageEffect::Direct { subject, kind } => {
                    let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                        return Ok(None);
                    };

                    DependencyStorageEffect::Direct {
                        subject,
                        kind: match kind.try_map_type(|ty| self.type_id(ty).ok_or(())) {
                            Ok(kind) => kind,
                            Err(()) => return Ok(None),
                        },
                    }
                }
                DependencyStorageEffect::Indirect {
                    reference,
                    projections,
                    kind,
                } => {
                    let (Some(reference), Some(projections)) = (
                        self.convert_dependency_subject(reference, symbols)?,
                        self.convert_dependency_projections(projections, symbols)?,
                    ) else {
                        return Ok(None);
                    };

                    DependencyStorageEffect::Indirect {
                        reference,
                        projections: projections.into(),
                        kind: match kind.try_map_type(|ty| self.type_id(ty).ok_or(())) {
                            Ok(kind) => kind,
                            Err(()) => return Ok(None),
                        },
                    }
                }
            });
        }

        Ok(Some(effects))
    }

    pub(super) fn convert_dependency_guard(
        &self,
        input: &InterfaceDependencyGuard,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<DependencyGuard>, InterfaceSemanticInternError> {
        Ok(match input {
            InterfaceDependencyGuard::NullablePresent(subject) => {
                let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                    return Ok(None);
                };

                Some(DependencyGuard::NullablePresent(subject))
            }
            InterfaceDependencyGuard::ActiveUnionVariant { subject, variant } => {
                let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                    return Ok(None);
                };

                Some(DependencyGuard::ActiveUnionVariant {
                    subject,
                    variant: resolve_exact(symbols, variant)?,
                })
            }
        })
    }
}
