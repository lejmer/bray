use super::bundle::section;
use super::model::EncodedSemanticSection;
use super::value::write_tagged_id;
use crate::semantic::codec::common::{
    REFERENCE_EXACT, REFERENCE_INPUT_ABSENT, REFERENCE_KIND_SHIFT, write_count, write_optional_u32,
    write_symbol_reference, write_symbol_references,
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
            encoder.write_u32(u32::from(contract.normal_storage_preserved));
            write_count(encoder, contract.requirements.len());

            for requirement in &*contract.requirements {
                encode_dependency_requirement(encoder, requirement);
            }

            for reports in [&contract.completion_reports, &contract.abnormal_reports] {
                encoder.write_u32(u32::from(reports.is_some()));

                if let Some(requirements) = reports {
                    write_count(encoder, requirements.len());

                    for requirement in requirements.iter() {
                        encode_dependency_requirement(encoder, requirement);
                    }
                }
            }

            write_count(encoder, contract.borrows.len());

            for relation in &*contract.borrows {
                let mut attributes = relation.kind().to_wire() << REFERENCE_KIND_SHIFT;

                if relation.is_exact() {
                    attributes |= REFERENCE_EXACT;
                }

                if relation.entry().is_none() {
                    attributes |= REFERENCE_INPUT_ABSENT;
                }

                encoder.write_u32(attributes);

                if let Some(entry) = relation.entry() {
                    encode_dependency_subject(encoder, entry);
                }

                encode_dependency_subject(encoder, relation.exit());

                if relation.entry().is_some() {
                    encoder.write_u32(relation.dereferences());
                    encode_dependency_projections(encoder, relation.target_projections());
                }
            }

            write_optional_u32(encoder, contract.returned_input.map(|input| input.raw()));
            write_optional_u32(encoder, contract.observed_result.map(|term| term.raw()));
            write_count(encoder, contract.carriers.len());

            for carrier in &*contract.carriers {
                encoder.write_u32(carrier.guarantee().raw());
                encode_dependency_projections(encoder, carrier.path());
            }

            write_count(encoder, contract.effects.len());

            for effect in &*contract.effects {
                use bray_symbols::DependencyStorageEffect;

                match effect {
                    DependencyStorageEffect::Unknown => encoder.write_u32(0),
                    DependencyStorageEffect::Ambient => encoder.write_u32(6),
                    DependencyStorageEffect::Escape => encoder.write_u32(5),
                    DependencyStorageEffect::Atomic => encoder.write_u32(4),
                    DependencyStorageEffect::Cleanup(ty) => {
                        encoder.write_u32(3);
                        encoder.write_u32(ty.raw());
                    }
                    DependencyStorageEffect::Direct { subject, kind } => {
                        encoder.write_u32(1);
                        encode_dependency_subject(encoder, subject);
                        encode_mutation(encoder, *kind);
                    }
                    DependencyStorageEffect::Indirect {
                        reference,
                        projections,
                        kind,
                    } => {
                        encoder.write_u32(2);
                        encode_dependency_subject(encoder, reference);
                        encode_dependency_projections(encoder, projections);
                        encode_mutation(encoder, *kind);
                    }
                }
            }
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
}

pub(super) fn encode_dependency_requirement(
    encoder: &mut WireEncoder,
    requirement: &InterfaceDependencyRequirement,
) {
    match &requirement.value {
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

pub(super) fn encode_dependency_subject(
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
    }

    encode_dependency_projections(encoder, &subject.projections);
}

fn encode_dependency_projections(
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
    use bray_symbols::DependencyMutationKind;

    encoder.write_u32(match kind {
        DependencyMutationKind::Write => 1,
        DependencyMutationKind::Consume => 2,
        DependencyMutationKind::Retire => 3,
        DependencyMutationKind::Atomic => 4,
        DependencyMutationKind::Replace(_) => 5,
        DependencyMutationKind::Clear => 6,
        DependencyMutationKind::Escape => 7,
    });

    if let DependencyMutationKind::Replace(ty) = kind {
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
