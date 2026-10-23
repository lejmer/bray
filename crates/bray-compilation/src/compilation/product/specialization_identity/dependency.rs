use super::encoding::StructuralValueEncoder;
use crate::fact::FactQueryError;
use bray_base::StableDigestHasher;
use bray_symbols::{
    CallablePhaseBehavior, DependencyContractTemplateId, DependencyGuard, DependencyProjection,
    DependencyRequirement, DependencySubject, DependencySubjectRoot,
};
use std::hash::Hasher;

impl StructuralValueEncoder<'_, '_> {
    pub(super) fn callable_phase_behavior(
        &mut self,
        behavior: &CallablePhaseBehavior,
    ) -> Result<(), FactQueryError> {
        for predicates in [
            behavior.predicate_requirements(),
            behavior.predicate_guarantees(),
        ] {
            self.predicate_summaries(predicates)?;
        }

        self.length(behavior.execution_properties().len());

        for property in behavior.execution_properties() {
            self.tag(match property {
                bray_symbols::ExecutionProperty::Pure => 0,
                bray_symbols::ExecutionProperty::Total => 1,
            });
        }

        self.length(behavior.effects().len());

        for requirement in behavior.effects() {
            self.symbol(requirement.declaration())?;
        }

        self.length(behavior.capabilities().len());

        for requirement in behavior.capabilities() {
            self.symbol(requirement.declaration())?;
        }

        self.length(behavior.trusted_capabilities().len());

        for requirement in behavior.trusted_capabilities() {
            self.ordinal(requirement.ordinal());
            self.symbol(requirement.capability().into())?;
        }

        self.length(behavior.execution_requirements().len());

        for requirement in behavior.execution_requirements() {
            self.symbol(requirement.declaration())?;
        }

        self.length(behavior.lifecycle_obligations().len());

        for obligation in behavior.lifecycle_obligations() {
            self.lifecycle_obligation(*obligation);
        }

        self.dependency_contract(behavior.dependency_contract())?;
        self.current_run_cancellation(behavior.current_run_cancellation());
        self.tag(u8::from(behavior.may_unwind()));

        let cleanup = behavior
            .receiver_cleanup()
            .iter()
            .map(|ty| Self::type_identity(self.values, self.binding_context, *ty))
            .collect::<Result<Vec<_>, _>>()?;

        self.unordered_identities(cleanup);

        Ok(())
    }

    fn predicate_summaries(
        &mut self,
        predicates: &[bray_symbols::PredicateSemanticSummary],
    ) -> Result<(), FactQueryError> {
        let mut identities = Vec::with_capacity(predicates.len());

        for predicate in predicates {
            let mut encoder = StructuralValueEncoder {
                values: self.values,
                binding_context: self.binding_context,
                digest: StableDigestHasher::new(),
                allow_static_address: self.allow_static_address,
            };

            encoder.tag(match predicate.domain() {
                bray_symbols::PredicateDomain::Value => 0,
                bray_symbols::PredicateDomain::Trusted => 1,
                bray_symbols::PredicateDomain::Execution => 2,
            });

            encoder.dependency_contract(predicate.dependency_contract())?;
            encoder.tag(u8::from(predicate.condition().is_some()));

            if let Some(condition) = predicate.condition() {
                encoder.constant_term(condition)?;
            }

            identities.push(encoder.digest.finalize());
        }

        self.unordered_identities(identities);

        Ok(())
    }

    pub(super) fn dependency_contract(
        &mut self,
        id: DependencyContractTemplateId,
    ) -> Result<(), FactQueryError> {
        let data = self.values.dependency_contract_template_data(id);

        self.digest.write_u8(u8::from(data.normal_completion()));

        self.digest
            .write_u8(u8::from(data.preserves_storage_on_normal_return()));

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

        self.digest
            .write_u8(u8::from(data.observed_result().is_some()));

        if let Some(term) = data.observed_result() {
            self.constant_term(term)?;
        }

        self.dependency_set(data.borrows(), Self::dependency_borrow)?;
        self.dependency_set(data.carriers(), Self::dependency_carrier)?;

        self.dependency_set(data.effects(), Self::dependency_effect)
    }

    pub(in crate::compilation) fn dependency_borrow(
        &mut self,
        relation: &bray_symbols::DependencyBorrowRelation,
    ) -> Result<(), FactQueryError> {
        self.boolean(relation.entry().is_some());

        if let Some(entry) = relation.entry() {
            self.dependency_subject(entry)?;
        }

        self.dependency_subject(relation.exit())?;
        self.digest.write_u32(relation.dereferences());
        self.boolean(relation.is_exact());
        self.dependency_projections(relation.target_projections())?;

        self.tag(match relation.kind() {
            bray_symbols::DependencyReferenceKind::Borrow => 1,
            bray_symbols::DependencyReferenceKind::Address => 2,
        });

        Ok(())
    }

    pub(in crate::compilation) fn dependency_carrier(
        &mut self,
        carrier: &bray_symbols::DependencyWitnessCarrier,
    ) -> Result<(), FactQueryError> {
        self.constant_term(carrier.guarantee())?;

        self.dependency_projections(carrier.path())
    }

    pub(in crate::compilation) fn dependency_effect(
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
            DependencyStorageEffect::Ambient => {
                self.tag(6);

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
        self.dependency_set(requirements, Self::dependency_requirement)
    }

    fn dependency_set<T>(
        &mut self,
        items: &[T],
        encode: impl Fn(&mut Self, &T) -> Result<(), FactQueryError>,
    ) -> Result<(), FactQueryError> {
        let identities = items
            .iter()
            .map(|item| self.item_identity(item, &encode))
            .collect::<Result<Vec<_>, _>>()?;

        self.unordered_identities(identities);

        Ok(())
    }

    pub(super) fn item_identity<T>(
        &self,
        item: &T,
        encode: impl FnOnce(&mut Self, &T) -> Result<(), FactQueryError>,
    ) -> Result<[u8; 32], FactQueryError> {
        let mut encoder = Self {
            values: self.values,
            binding_context: self.binding_context,
            digest: StableDigestHasher::new(),
            allow_static_address: self.allow_static_address,
        };

        encode(&mut encoder, item)?;

        Ok(encoder.digest.finalize())
    }

    pub(in crate::compilation) fn dependency_requirement(
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
                output,
                callable,
                requirement,
                inputs,
            } => {
                self.tag(if requirement.is_some() { 2 } else { 3 });

                self.tag(match output {
                    bray_symbols::DependencyCallOutput::Value => 0,
                    bray_symbols::DependencyCallOutput::CompletionReport => 1,
                    bray_symbols::DependencyCallOutput::AbnormalReport => 2,
                });

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
