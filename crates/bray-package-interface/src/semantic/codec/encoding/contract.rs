use bray_symbols::DependencyMutationKind;
use bray_symbols::DependencyStorageEffect;
use super::bundle::section;
use super::model::EncodedSemanticSection;
use super::value::write_tagged_id;
use crate::semantic::codec::common::{
    REFERENCE_ALLOCATION, REFERENCE_BACKING_REGION, REFERENCE_EMPTY, REFERENCE_EXACT, REFERENCE_INPUT_ABSENT, REFERENCE_INPUT_REGIONS, REFERENCE_KIND_SHIFT, REFERENCE_NO_PAYLOAD,
    write_count, write_optional_u32, write_symbol_reference, write_symbol_references,
};
use crate::semantic::codec::record::encode_record_table;
use crate::semantic::model::{
    InterfaceCallableContractClause, InterfaceCallableContractClauseValue,
    InterfaceCallablePhaseBehavior, InterfaceConstraintKind, InterfaceDependencyGuard,
    InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementKind, InterfaceDependencyRequirementValue,
    InterfaceDependencySubject, InterfaceDependencySubjectRoot,
};
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemantics};
use bray_symbols::BorrowKind;

pub(in crate::semantic::codec) fn encode_dependency_effects(
    encoder: &mut WireEncoder,
    effects: &[crate::InterfaceStorageEffect],
) {
    write_count(encoder, effects.len());

    for effect in effects {
        encode_dependency_effect(encoder, effect);
    }
}

fn encode_dependency_effect(encoder: &mut WireEncoder, effect: &crate::InterfaceStorageEffect) {
    match effect {
        DependencyStorageEffect::OrderedRetirement { retirement, discharged } => {
            encoder.write_u32(10);
            encode_dependency_effect(encoder, retirement);
            write_count(encoder, discharged.len());

            for duty in discharged.iter() {
                encode_dependency_effect(encoder, duty);
            }
        }
        DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient } => {
            encoder.write_u32(7);
            encode_mutation(encoder, *kind);
            write_optional_u32(encoder, access_type.map(|ty| ty.raw()));
            encoder.write_u32(u32::from(*ambient));
        }
        DependencyStorageEffect::Unknown => encoder.write_u32(0),
        DependencyStorageEffect::Ambient => encoder.write_u32(6),
        DependencyStorageEffect::Escape => encoder.write_u32(5),
        DependencyStorageEffect::Atomic => encoder.write_u32(4),
        DependencyStorageEffect::Direct { addressed, subject, kind, access_type } => {
            encoder.write_u32(if *addressed { 1 } else { 8 });
            encode_dependency_subject(encoder, subject);
            encode_mutation(encoder, *kind);
            write_optional_u32(encoder, access_type.map(|ty| ty.raw()));
        }
        DependencyStorageEffect::Indirect { addressed, reads, reference, projections, kind, access_type } => {
            encoder.write_u32(if *addressed { 2 } else { 9 });
            encode_dependency_subject(encoder, reference);
            write_count(encoder, reads.len());

            for index in reads.iter() {
                encoder.write_u32(*index);
            }

            encode_dependency_projections(encoder, projections);
            encode_mutation(encoder, *kind);
            write_optional_u32(encoder, access_type.map(|ty| ty.raw()));
        }
    }
}

pub(super) fn encode_contracts(semantics: &InterfaceSemantics) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encode_record_table(
        &mut encoder,
        &semantics.dependency_contracts,
        |encoder, contract| {
            write_count(encoder, contract.synchronous_cleanup.len());

            for requirement in &*contract.synchronous_cleanup {
                encode_dependency_requirement(encoder, requirement);
            }

            encoder.write_u32(u32::from(contract.normal_completion));
            encode_dependency_requirements(encoder, &contract.requirements);

            encode_failure_outcomes(encoder, contract.failure_outcomes.as_deref());

            super::super::captured::encode_values(encoder, &contract.captured_values);

            encode_dependency_effects(encoder, &contract.effects);
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.constraints,
        |encoder, constraint| {
            write_symbol_reference(encoder, &constraint.owner);
            encoder.write_u32(constraint.ordinal.raw());

            match constraint.kind {
                InterfaceConstraintKind::Predicate(predicate) => {
                    encoder.write_u32(1);
                    encode_predicate_summary(encoder, predicate);
                }
                InterfaceConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    encoder.write_u32(2);
                    encoder.write_u32(subject.raw());
                    encoder.write_u32(application.raw());
                }
                InterfaceConstraintKind::TypeEquality { left, right } => {
                    encoder.write_u32(3);
                    encoder.write_u32(left.raw());
                    encoder.write_u32(right.raw());
                }
            }
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.callable_contracts,
        |encoder, contract| {
            write_symbol_reference(encoder, &contract.owner);
            encode_callable_clauses(encoder, &contract.invocation_preconditions);
            encode_callable_clauses(encoder, &contract.static_constraints);
            encode_callable_clauses(encoder, &contract.normal_completion_postconditions);
            super::execution::encode_execution_contract(encoder, &contract.execution_contract);
            encode_callable_behavior(encoder, &contract.invocation_behavior);

            match &contract.deferred_execution_behavior {
                Some(behavior) => {
                    encoder.write_u32(1);
                    encode_callable_behavior(encoder, behavior);
                }
                None => encoder.write_u32(0),
            }
        },
    );

    section(
        InterfaceSectionTag::Contracts,
        semantics.dependency_contracts.len()
            + semantics.constraints.len()
            + semantics.callable_contracts.len(),
        encoder,
    )
}

fn encode_failure_outcomes(encoder: &mut WireEncoder, outcomes: Option<&[crate::InterfaceFailureOutcome]>) {
    encoder.write_u32(u32::from(outcomes.is_some()));

    if let Some(outcomes) = outcomes {
        write_count(encoder, outcomes.len());

        for row in outcomes {
            encoder.write_u32(row.kind.map_or(0, |kind| kind as u32));
            write_optional_u32(encoder, row.guard.map(|term| term.raw()));
            encode_optional_requirements(encoder, row.report.as_deref());

            encode_optional_effects(encoder, row.effects.as_deref());
            encode_optional_effects(encoder, row.cell_changes.as_deref());
            super::super::captured::encode_bindings(encoder, &row.writes);
        }
    }
}

pub(in crate::semantic::codec) fn encode_optional_effects(encoder: &mut WireEncoder, effects: Option<&[crate::InterfaceStorageEffect]>) {
    encoder.write_u32(u32::from(effects.is_some()));

    if let Some(effects) = effects {
        encode_dependency_effects(encoder, effects);
    }
}

fn encode_callable_clauses(encoder: &mut WireEncoder, clauses: &[InterfaceCallableContractClause]) {
    write_count(encoder, clauses.len());

    for clause in clauses {
        encoder.write_u32(clause.ordinal.raw());

        match clause.value {
            InterfaceCallableContractClauseValue::Predicate(predicate) => {
                encoder.write_u32(1);
                encode_predicate_summary(encoder, predicate);
            }
            InterfaceCallableContractClauseValue::TraitSatisfaction {
                subject,
                application,
            } => {
                encoder.write_u32(2);
                encoder.write_u32(subject.raw());
                encoder.write_u32(application.raw());
            }
        }
    }
}

pub(super) fn encode_callable_behavior(
    encoder: &mut WireEncoder,
    behavior: &InterfaceCallablePhaseBehavior,
) {
    for predicates in [
        &behavior.predicate_requirements,
        &behavior.predicate_guarantees,
    ] {
        write_count(encoder, predicates.len());

        for predicate in &**predicates {
            encode_predicate_summary(encoder, *predicate);
        }
    }

    for requirements in [
        &behavior.effects,
        &behavior.capabilities,
        &behavior.execution_requirements,
    ] {
        write_symbol_references(encoder, requirements);
    }

    write_count(encoder, behavior.trusted_capabilities.len());

    for requirement in &*behavior.trusted_capabilities {
        encoder.write_u32(requirement.ordinal.raw());
        write_symbol_reference(encoder, &requirement.capability);
    }

    write_count(encoder, behavior.lifecycle_obligations.len());

    for obligation in &*behavior.lifecycle_obligations {
        encoder.write_u32(obligation.to_wire());
    }

    encoder.write_u32(behavior.dependency_contract.raw());
    encoder.write_u32(behavior.current_run_cancellation.to_wire());
    encoder.write_u32(u32::from(behavior.may_unwind));

    write_count(encoder, behavior.receiver_cleanup.len());

    for ty in &*behavior.receiver_cleanup {
        encoder.write_u32(ty.raw());
    }

    write_count(encoder, behavior.execution_properties.len());

    for property in &*behavior.execution_properties {
        encoder.write_u32(property.to_wire());
    }

    super::execution::encode_execution_domains(
        encoder,
        &behavior.execution_domains,
        |encoder, predicate| encode_predicate_summary(encoder, *predicate),
    );
}

pub(in crate::semantic::codec) fn encode_optional_requirements(
    encoder: &mut WireEncoder,
    requirements: Option<&[InterfaceDependencyRequirement]>,
) {
    encoder.write_u32(u32::from(requirements.is_some()));

    if let Some(requirements) = requirements {
        encode_dependency_requirements(encoder, requirements);
    }
}

pub(in crate::semantic::codec) fn encode_dependency_requirements(
    encoder: &mut WireEncoder,
    requirements: &[InterfaceDependencyRequirement],
) {
    write_count(encoder, requirements.len());

    for requirement in requirements {
        encode_dependency_requirement(encoder, requirement);
    }
}

pub(super) fn encode_dependency_requirement(
    encoder: &mut WireEncoder,
    requirement: &InterfaceDependencyRequirement,
) {
    match &requirement.value {
        InterfaceDependencyRequirementValue::CleanupDependencies { ty, requirements } => {
            encoder.write_u32(10);
            encoder.write_u32(ty.raw());
            write_count(encoder, requirements.len());

            for nested in requirements.iter() {
                encode_dependency_requirement(encoder, nested);
            }
        }
        InterfaceDependencyRequirementValue::SynchronousCleanup(ty) => {
            encoder.write_u32(7);
            encoder.write_u32(ty.raw());
        }
        InterfaceDependencyRequirementValue::FinalizerBodyCleanup(ty) => {
            encoder.write_u32(8);
            encoder.write_u32(ty.raw());
        }
        InterfaceDependencyRequirementValue::DefaultCleanup {
            provider,
            substitution,
            contextual_self,
        } => {
            encoder.write_u32(9);
            write_symbol_reference(encoder, provider);
            encoder.write_u32(substitution.raw());

            write_optional_u32(
                encoder,
                contextual_self.as_ref().map(|(_, subject)| subject.raw()),
            );

            if let Some((context, _)) = contextual_self {
                write_symbol_reference(encoder, context);
            }
        }
        InterfaceDependencyRequirementValue::Variable { depth, ordinal } => {
            encoder.write_u32(5);
            encoder.write_u32(*depth);
            encoder.write_u32(ordinal.raw());
        }
        InterfaceDependencyRequirementValue::FixedPoint {
            definitions,
            result,
        } => {
            encoder.write_u32(6);
            write_count(encoder, definitions.len());

            for definition in definitions.iter().chain(std::iter::once(result)) {
                write_count(encoder, definition.len());

                for nested in definition.iter() {
                    encode_dependency_requirement(encoder, nested);
                }
            }
        }
        InterfaceDependencyRequirementValue::ResultCall {
            output,
            callable,
            requirement,
            inputs,
        } => {
            encoder.write_u32(if requirement.is_some() { 3 } else { 4 });
            encoder.write_u32(output.to_wire());
            encoder.write_u32(callable.raw());

            if let Some((subject, application)) = requirement {
                encoder.write_u32(subject.raw());
                encoder.write_u32(application.raw());
            }

            encode_dependency_call_inputs(encoder, inputs);
        }
        InterfaceDependencyRequirementValue::Direct { subject, kind } => {
            encoder.write_u32(1);

            encode_dependency_subject(encoder, subject);
            encode_dependency_requirement_kind(encoder, *kind);
        }
        InterfaceDependencyRequirementValue::Guarded {
            guard,
            requirements,
        } => {
            encoder.write_u32(2);

            encode_dependency_guard(encoder, guard);
            write_count(encoder, requirements.len());

            for nested in &**requirements {
                encode_dependency_requirement(encoder, nested);
            }
        }
    }
}

fn encode_dependency_call_inputs(
    encoder: &mut WireEncoder,
    inputs: &[crate::InterfaceDependencyCallInput],
) {
    write_count(encoder, inputs.len());

    for input in inputs.iter() {
        encode_dependency_subject(
            encoder,
            &InterfaceDependencySubject::new(input.root.clone(), []),
        );

        for requirements in [&input.values, &input.storage] {
            write_count(encoder, requirements.len());

            for nested in requirements.iter() {
                encode_dependency_requirement(encoder, nested);
            }
        }
    }
}

pub(in crate::semantic::codec) fn encode_dependency_storage(
    encoder: &mut WireEncoder,
    references: &[bray_symbols::DependencyBorrowRelation<crate::InterfaceDependencySubject, crate::InterfaceDependencyProjection>],
    stored_values: &[bray_symbols::DependencyStoredValue<crate::InterfaceDependencySubject>],
) {
    write_count(encoder, references.len());

    for relation in references {
        let mut attributes = relation.kind().to_wire() << REFERENCE_KIND_SHIFT;

        if relation.is_exact() {
            attributes |= REFERENCE_EXACT;
        }

        if relation.provenance()
            == bray_symbols::DependencyReferenceProvenance::InputRegions
        {
            attributes |= REFERENCE_INPUT_REGIONS;
        }

        if relation.source_subject().is_none() {
            attributes |= REFERENCE_INPUT_ABSENT;
        }

        if relation.allocation().is_some() { attributes |= REFERENCE_ALLOCATION; }
        if relation.is_empty_reference() { attributes |= REFERENCE_EMPTY; }
        if relation.is_no_payload() { attributes |= REFERENCE_NO_PAYLOAD; }
        if relation.is_backing_region() { attributes |= REFERENCE_BACKING_REGION; }

        encoder.write_u32(attributes);

        if let Some(entry) = relation.source_subject() {
            encode_dependency_subject(encoder, entry);
        }

        encode_dependency_subject(encoder, relation.exit());

        if relation.source_subject().is_some() {
            encoder.write_u32(relation.dereferences());
            encode_dependency_projections(encoder, relation.target_projections());
        }
    }

    write_count(encoder, stored_values.len());

    for value in stored_values {
        encode_dependency_subject(encoder, value.destination().subject());

        encoder.write_u32(value.destination().dereferences());
        encoder.write_u32(u32::from(value.is_complete()));

        write_count(encoder, value.sources().len());

        for source in value.sources() {
            encode_dependency_subject(encoder, source.subject());

            encoder.write_u32(source.dereferences());
        }
    }
}

pub(in crate::semantic::codec) fn encode_dependency_subject(
    encoder: &mut WireEncoder,
    subject: &InterfaceDependencySubject,
) {
    match &subject.root {
        InterfaceDependencySubjectRoot::Receiver => encoder.write_u32(1),
        InterfaceDependencySubjectRoot::Parameter(ordinal) => {
            write_tagged_id(encoder, 2, ordinal.raw());
        }
        InterfaceDependencySubjectRoot::Result => encoder.write_u32(3),
        InterfaceDependencySubjectRoot::EvaluationStorage => encoder.write_u32(8),
        InterfaceDependencySubjectRoot::ScopedCapability(ordinal) => {
            write_tagged_id(encoder, 4, ordinal.raw());
        }
        InterfaceDependencySubjectRoot::ImplementationWitness(instance) => {
            write_tagged_id(encoder, 5, instance.raw());
        }
        InterfaceDependencySubjectRoot::ImplementationRequirement(subject, application) => {
            encoder.write_u32(9);
            encoder.write_u32(subject.raw());
            encoder.write_u32(application.raw());
        }
        InterfaceDependencySubjectRoot::ProductStatic(reference) => {
            encoder.write_u32(6);
            write_symbol_reference(encoder, reference);
        }
        InterfaceDependencySubjectRoot::ExactThreadStatic(reference) => {
            encoder.write_u32(7);
            write_symbol_reference(encoder, reference);
        }
        InterfaceDependencySubjectRoot::StaticInstance(reference, duration) => {
            encoder.write_u32(match duration {
                bray_symbols::StaticStorageDuration::Product => 10,
                bray_symbols::StaticStorageDuration::ExactThread => 11,
            });

            super::super::captured::encode_static_reference(encoder, reference);
        }
    }

    encode_dependency_projections(encoder, &subject.projections);
}

pub(in crate::semantic::codec) fn encode_dependency_projections(
    encoder: &mut WireEncoder,
    projections: &[InterfaceDependencyProjection],
) {
    write_count(encoder, projections.len());

    for projection in projections {
        match projection {
            InterfaceDependencyProjection::ProductField(field) => {
                encoder.write_u32(1);
                write_symbol_reference(encoder, field);
            }
            InterfaceDependencyProjection::TupleElement(ordinal) => {
                write_tagged_id(encoder, 2, ordinal.raw());
            }
            InterfaceDependencyProjection::Element(term) => {
                write_tagged_id(encoder, 3, term.raw());
            }
            InterfaceDependencyProjection::NullableValue => encoder.write_u32(4),
            InterfaceDependencyProjection::UnionPayloadField(field) => {
                encoder.write_u32(5);
                write_symbol_reference(encoder, field);
            }
            InterfaceDependencyProjection::OwnedTarget => encoder.write_u32(6),
            InterfaceDependencyProjection::UninitPayload => encoder.write_u32(7),
            InterfaceDependencyProjection::AnyElement => encoder.write_u32(8),
            InterfaceDependencyProjection::AddressTarget => encoder.write_u32(9),
        }
    }
}

pub(super) fn encode_dependency_guard(encoder: &mut WireEncoder, guard: &InterfaceDependencyGuard) {
    match guard {
        InterfaceDependencyGuard::NullablePresent(subject) => {
            encoder.write_u32(1);
            encode_dependency_subject(encoder, subject);
        }
        InterfaceDependencyGuard::ActiveUnionVariant { subject, variant } => {
            encoder.write_u32(2);
            encode_dependency_subject(encoder, subject);
            write_symbol_reference(encoder, variant);
        }
    }
}

pub(super) fn encode_dependency_requirement_kind(
    encoder: &mut WireEncoder,
    kind: InterfaceDependencyRequirementKind,
) {
    match kind {
        InterfaceDependencyRequirementKind::StorageAlive => encoder.write_u32(1),
        InterfaceDependencyRequirementKind::ValueDependencies => encoder.write_u32(7),
        InterfaceDependencyRequirementKind::StorageInitialized => encoder.write_u32(2),
        InterfaceDependencyRequirementKind::ExclusiveMutationAuthority => encoder.write_u32(3),
        InterfaceDependencyRequirementKind::BorrowCapabilityActive(kind) => {
            encoder.write_u32(4);

            encoder.write_u32(match kind {
                BorrowKind::Shared => 1,
                BorrowKind::Mutable => 2,
            });
        }
        InterfaceDependencyRequirementKind::ScopedCapabilityLive => encoder.write_u32(5),
        InterfaceDependencyRequirementKind::LifecycleObligation(kind) => {
            encoder.write_u32(6);
            encoder.write_u32(kind.to_wire());
        }
    }
}

fn encode_mutation(
    encoder: &mut WireEncoder,
    kind: bray_symbols::DependencyMutationKind<crate::InterfaceTypeId>,
) {

    encoder.write_u32(match kind {
        DependencyMutationKind::Write => 1,
        DependencyMutationKind::Consume => 2,
        DependencyMutationKind::Retire => 3,
        DependencyMutationKind::Atomic => 4,
        DependencyMutationKind::Replace(_) => 5,
        DependencyMutationKind::Clear => 6,
        DependencyMutationKind::Escape => 7,
        DependencyMutationKind::Cleanup(_) => 8,
        DependencyMutationKind::Dispose(_) => 9,
        DependencyMutationKind::Discharge(_) => 10,
        DependencyMutationKind::DisposeCaptured { include_inline: false, .. } => 11,
        DependencyMutationKind::DisposeCaptured { include_inline: true, .. } => 12,
    });

    if let Some(ty) = kind.cleanup_type().or(match &kind {
        DependencyMutationKind::Replace(ty) | DependencyMutationKind::Discharge(ty) => Some(ty),
        _ => None,
    }) {
        encoder.write_u32(ty.raw());
    }
}

pub(super) fn encode_predicate_summary(
    encoder: &mut WireEncoder,
    predicate: crate::InterfacePredicateSummary,
) {
    encoder.write_u32(predicate.dependency_contract.raw());
    write_optional_u32(encoder, predicate.condition.map(|term| term.raw()));

    encoder.write_u32(match predicate.domain {
        bray_symbols::PredicateDomain::Value => 0,
        bray_symbols::PredicateDomain::Trusted => 1,
        bray_symbols::PredicateDomain::Execution => 2,
    });
}
