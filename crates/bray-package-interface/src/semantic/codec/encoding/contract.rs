use bray_symbols::CallableConditions;

use super::bundle::section;
use super::model::EncodedSemanticSection;
use super::value::write_tagged_id;
use crate::semantic::codec::common::{
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

pub(super) fn encode_contracts(semantics: &InterfaceSemantics) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encode_record_table(
        &mut encoder,
        &semantics.dependency_contracts,
        |encoder, contract| {
            write_count(encoder, contract.requirements.len());

            for requirement in &*contract.requirements {
                encode_dependency_requirement(encoder, requirement);
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
                    encode_predicate(encoder, predicate);
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

            encode_callable_conditions(encoder, contract.conditions());
            write_count(encoder, contract.evidence().len());

            for proof in contract.evidence() {
                encoder.write_u32(match proof.origin() {
                    bray_symbols::CallableEvidenceOrigin::CheckedBody => 0,
                    bray_symbols::CallableEvidenceOrigin::ForeignAssertion => 1,
                });

                encode_contract_obligation(encoder, proof.obligation());
                write_count(encoder, proof.dependencies().len());

                for (target, obligation) in proof.dependencies() {
                    encoder.write_u32(target.callable().raw());

                    match target.dispatch() {
                        Some((subject, application)) => {
                            encoder.write_u32(1);
                            encoder.write_u32(subject.raw());
                            encoder.write_u32(application.raw());
                        }
                        None => encoder.write_u32(0),
                    }

                    encode_contract_obligation(encoder, *obligation);
                }
            }

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

fn encode_contract_obligation(
    encoder: &mut WireEncoder,
    obligation: bray_symbols::CallableContractObligation,
) {
    match obligation {
        bray_symbols::CallableContractObligation::Execution(guarantee) => {
            encoder.write_u32(0);
            encoder.write_u32(guarantee.property().to_wire());

            write_optional_u32(
                encoder,
                guarantee.guard().map(bray_symbols::SymbolOrdinal::raw),
            );
        }
        bray_symbols::CallableContractObligation::Postcondition(ordinal) => {
            encoder.write_u32(1);
            encoder.write_u32(ordinal.raw());
        }
    }
}

pub(super) fn encode_callable_conditions(
    encoder: &mut WireEncoder,
    conditions: &bray_symbols::CallableConditionSet<InterfaceCallableContractClause>,
) {
    encode_callable_clauses(encoder, conditions.invocation_preconditions());
    encode_callable_clauses(encoder, conditions.static_constraints());
    encode_callable_clauses(encoder, conditions.normal_completion_postconditions());
    encode_callable_clauses(encoder, conditions.entry_guards());
    encode_callable_clauses(encoder, conditions.guarded_postconditions());

    write_count(encoder, conditions.execution_guarantees().len());

    for guarantee in conditions.execution_guarantees() {
        encoder.write_u32(guarantee.property().to_wire());

        write_optional_u32(
            encoder,
            guarantee.guard().map(bray_symbols::SymbolOrdinal::raw),
        );
    }
}

fn encode_predicate(encoder: &mut WireEncoder, predicate: crate::InterfacePredicateSummary) {
    encoder.write_u32(predicate.dependency_contract.raw());

    write_optional_u32(
        encoder,
        predicate.condition.map(crate::InterfaceConstantTermId::raw),
    );
}

fn encode_callable_clauses(encoder: &mut WireEncoder, clauses: &[InterfaceCallableContractClause]) {
    write_count(encoder, clauses.len());

    for clause in clauses {
        encoder.write_u32(clause.ordinal.raw());

        write_optional_u32(encoder, clause.guard.map(bray_symbols::SymbolOrdinal::raw));

        match clause.value {
            InterfaceCallableContractClauseValue::Predicate(predicate) => {
                encoder.write_u32(1);
                encode_predicate(encoder, predicate);
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
}

pub(super) fn encode_dependency_requirement(
    encoder: &mut WireEncoder,
    requirement: &InterfaceDependencyRequirement,
) {
    match &requirement.value {
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
        InterfaceDependencySubjectRoot::ScopedCapability(ordinal) => {
            write_tagged_id(encoder, 4, ordinal.raw());
        }
        InterfaceDependencySubjectRoot::ImplementationWitness(instance) => {
            write_tagged_id(encoder, 5, instance.raw());
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

    write_count(encoder, subject.projections.len());

    for projection in &*subject.projections {
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
