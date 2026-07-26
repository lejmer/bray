use bray_symbols::{
    DependencyContractTemplateData, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencySubject, DependencySubjectRoot, SemanticValueStore,
};

use crate::{
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceSemanticFacts,
};

use super::common::{convert_requirement_kind, resolve_exact};
use super::{InterfaceSemanticInternError, InterfaceSymbolResolver, InternState};

impl InternState {
    pub(super) fn intern_dependency_contracts(
        &mut self,
        facts: &InterfaceSemanticFacts,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in facts.dependency_contracts.iter().enumerate() {
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

    pub(super) fn convert_dependency_subject(
        &self,
        input: &InterfaceDependencySubject,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<DependencySubject>, InterfaceSemanticInternError> {
        let root = match input.root {
            InterfaceDependencySubjectRoot::Receiver => DependencySubjectRoot::Receiver,
            InterfaceDependencySubjectRoot::Parameter(ordinal) => {
                DependencySubjectRoot::Parameter(ordinal)
            }
            InterfaceDependencySubjectRoot::Result => DependencySubjectRoot::Result,
            InterfaceDependencySubjectRoot::ScopedCapability(ordinal) => {
                DependencySubjectRoot::ScopedCapability(ordinal)
            }
            InterfaceDependencySubjectRoot::ImplementationWitness(id) => {
                let Some(id) = self.implementation_instance_id(id) else {
                    return Ok(None);
                };

                DependencySubjectRoot::ImplementationWitness(id)
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
