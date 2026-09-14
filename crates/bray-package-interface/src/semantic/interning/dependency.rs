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

            self.dependency_contracts[index] = Some(store.intern_dependency_contract_template(
                DependencyContractTemplateData::new(requirements),
            )?);
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

                    DependencyRequirement::result_call(callable, requirement, converted)
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

        let mut projections = Vec::with_capacity(input.projections.len());

        for projection in &*input.projections {
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
            };

            projections.push(projection);
        }

        Ok(Some(DependencySubject::new(root, projections)))
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
