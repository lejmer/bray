use super::encoding::StructuralValueEncoder;
use crate::fact::FactQueryError;
use bray_base::StableDigestHasher;
use bray_symbols::{
    DependencyContractTemplateId, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencySubject, DependencySubjectRoot,
};
use std::hash::Hasher;

impl StructuralValueEncoder<'_, '_> {
    pub(super) fn dependency_contract(
        &mut self,
        id: DependencyContractTemplateId,
    ) -> Result<(), FactQueryError> {
        let data = self.values.dependency_contract_template_data(id);

        self.digest.write_u8(u8::from(data.normal_completion()));
        self.dependency_requirements(data.requirements())?;

        for reports in [data.completion_reports(), data.abnormal_reports()] {
            self.digest.write_u8(u8::from(reports.is_some()));

            if let Some(requirements) = reports {
                self.dependency_requirements(requirements)?;
            }
        }

        self.digest
            .write_u8(u8::from(data.returned_input().is_some()));

        if let Some(input) = data.returned_input() {
            self.constant_term(input)?;
        }

        let mut identities = Vec::with_capacity(data.borrows().len());

        for relation in data.borrows() {
            let mut encoder = StructuralValueEncoder {
                values: self.values,
                binding_context: self.binding_context,
                digest: StableDigestHasher::new(),
                allow_static_address: self.allow_static_address,
            };

            encoder.dependency_subject(relation.entry())?;
            encoder.dependency_subject(relation.exit())?;
            encoder.digest.write_u32(relation.dereferences());
            encoder.dependency_projections(relation.target_projections())?;

            encoder.tag(match relation.kind() {
                bray_symbols::DependencyReferenceKind::Borrow => 1,
                bray_symbols::DependencyReferenceKind::Address => 2,
            });

            identities.push(encoder.digest.finalize());
        }

        self.unordered_identities(identities);

        let mut identities = Vec::with_capacity(data.carriers().len());

        for carrier in data.carriers() {
            let mut encoder = StructuralValueEncoder {
                values: self.values,
                binding_context: self.binding_context,
                digest: StableDigestHasher::new(),
                allow_static_address: self.allow_static_address,
            };

            encoder.constant_term(carrier.guarantee())?;
            encoder.dependency_projections(carrier.path())?;
            identities.push(encoder.digest.finalize());
        }

        self.unordered_identities(identities);

        let mut identities = Vec::with_capacity(data.effects().len());

        for effect in data.effects() {
            let mut encoder = StructuralValueEncoder {
                values: self.values,
                binding_context: self.binding_context,
                digest: StableDigestHasher::new(),
                allow_static_address: self.allow_static_address,
            };

            encoder.dependency_effect(effect)?;
            identities.push(encoder.digest.finalize());
        }

        self.unordered_identities(identities);

        Ok(())
    }

    fn dependency_effect(
        &mut self,
        effect: &bray_symbols::DependencyStorageEffect,
    ) -> Result<(), FactQueryError> {
        use bray_symbols::{DependencyMutationKind, DependencyStorageEffect};

        let kind = match effect {
            DependencyStorageEffect::Cleanup(ty) => {
                self.tag(3);

                return self.ty(*ty);
            }
            DependencyStorageEffect::Atomic => {
                self.tag(4);

                return Ok(());
            }
            DependencyStorageEffect::Escape => {
                self.tag(5);

                return Ok(());
            }
            DependencyStorageEffect::Unknown => {
                self.tag(0);

                return Ok(());
            }
            DependencyStorageEffect::Direct { subject, kind } => {
                self.tag(1);
                self.dependency_subject(subject)?;

                kind
            }
            DependencyStorageEffect::Indirect {
                reference,
                projections,
                kind,
            } => {
                self.tag(2);
                self.dependency_subject(reference)?;
                self.dependency_projections(projections)?;

                kind
            }
        };

        self.tag(match kind {
            DependencyMutationKind::Write => 1,
            DependencyMutationKind::Consume => 2,
            DependencyMutationKind::Retire => 3,
            DependencyMutationKind::Atomic => 4,
            DependencyMutationKind::Replace(_) => 5,
            DependencyMutationKind::Clear => 6,
            DependencyMutationKind::Escape => 7,
        });

        if let DependencyMutationKind::Replace(ty) = kind {
            self.ty(*ty)?;
        }

        Ok(())
    }

    pub(super) fn unordered_identities(&mut self, mut identities: Vec<[u8; 32]>) {
        identities.sort_unstable();
        self.length(identities.len());

        for identity in identities {
            self.digest.write(&identity);
        }
    }

    fn dependency_requirements(
        &mut self,
        requirements: &[DependencyRequirement],
    ) -> Result<(), FactQueryError> {
        let mut identities = Vec::with_capacity(requirements.len());

        for requirement in requirements {
            let mut encoder = StructuralValueEncoder {
                values: self.values,
                binding_context: self.binding_context,
                digest: StableDigestHasher::new(),
                allow_static_address: self.allow_static_address,
            };

            encoder.dependency_requirement(requirement)?;
            identities.push(encoder.digest.finalize());
        }

        // Requirement sets must not inherit store-local allocation order.
        self.unordered_identities(identities);

        Ok(())
    }

    fn dependency_requirement(
        &mut self,
        requirement: &DependencyRequirement,
    ) -> Result<(), FactQueryError> {
        match requirement {
            DependencyRequirement::Variable { depth, ordinal } => {
                self.tag(4);
                self.length(*depth as usize);
                self.length(ordinal.raw() as usize);
            }
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => {
                self.tag(5);
                self.length(definitions.len());

                for requirements in definitions
                    .iter()
                    .map(|definition| definition.as_ref())
                    .chain([result.as_ref()])
                {
                    self.dependency_requirements(requirements)?;
                }
            }
            DependencyRequirement::ResultCall {
                callable,
                requirement,
                inputs,
            } => {
                self.tag(if requirement.is_some() { 2 } else { 3 });
                self.callable_instance(*callable)?;

                if let Some(requirement) = requirement {
                    self.ty(requirement.subject())?;
                    self.trait_application(requirement.trait_application())?;
                }

                self.dependency_call_inputs(inputs)?;
            }
            DependencyRequirement::Direct { subject, kind } => {
                self.tag(0);
                self.dependency_subject(subject)?;
                self.dependency_requirement_kind(*kind);
            }
            DependencyRequirement::Guarded(guarded) => {
                self.tag(1);
                self.dependency_guard(guarded.guard())?;
                self.dependency_requirements(guarded.requirements())?;
            }
        }

        Ok(())
    }

    fn dependency_call_inputs(
        &mut self,
        inputs: &[bray_symbols::DependencyCallInput],
    ) -> Result<(), FactQueryError> {
        self.length(inputs.len());

        for input in inputs {
            self.dependency_subject(&DependencySubject::root(input.root()))?;

            for requirements in [input.values(), input.storage()] {
                self.dependency_requirements(requirements)?;
            }
        }

        Ok(())
    }

    fn dependency_guard(&mut self, guard: &DependencyGuard) -> Result<(), FactQueryError> {
        match guard {
            DependencyGuard::NullablePresent(subject) => {
                self.tag(0);
                self.dependency_subject(subject)?;
            }
            DependencyGuard::ActiveUnionVariant { subject, variant } => {
                self.tag(1);
                self.dependency_subject(subject)?;
                self.symbol((*variant).into())?;
            }
        }

        Ok(())
    }

    fn dependency_subject(&mut self, subject: &DependencySubject) -> Result<(), FactQueryError> {
        match subject.subject_root() {
            DependencySubjectRoot::Receiver => self.tag(0),
            DependencySubjectRoot::Parameter(ordinal) => {
                self.tag(1);
                self.ordinal(ordinal);
            }
            DependencySubjectRoot::Result => self.tag(2),
            DependencySubjectRoot::EvaluationStorage => self.tag(7),
            DependencySubjectRoot::ScopedCapability(ordinal) => {
                self.tag(3);
                self.ordinal(ordinal);
            }
            DependencySubjectRoot::ImplementationWitness(instance) => {
                self.tag(4);
                self.implementation_instance(instance)?;
            }
            DependencySubjectRoot::ProductStatic(id) => {
                self.tag(5);
                self.symbol(id.into())?;
            }
            DependencySubjectRoot::ExactThreadStatic(id) => {
                self.tag(6);
                self.symbol(id.into())?;
            }
        }

        self.dependency_projections(subject.projections())
    }

    fn dependency_projections(
        &mut self,
        projections: &[DependencyProjection],
    ) -> Result<(), FactQueryError> {
        self.length(projections.len());

        for projection in projections {
            match projection {
                DependencyProjection::ProductField(field) => {
                    self.tag(0);
                    self.symbol((*field).into())?;
                }
                DependencyProjection::TupleElement(ordinal) => {
                    self.tag(1);
                    self.ordinal(*ordinal);
                }
                DependencyProjection::Element(index) => {
                    self.tag(2);
                    self.constant_term(*index)?;
                }
                DependencyProjection::NullableValue => self.tag(3),
                DependencyProjection::UnionPayloadField(field) => {
                    self.tag(4);
                    self.symbol((*field).into())?;
                }
                DependencyProjection::OwnedTarget => self.tag(5),
                DependencyProjection::UninitPayload => self.tag(6),
            }
        }

        Ok(())
    }
}
