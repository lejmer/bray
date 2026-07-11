use super::model::EncodedSemanticSection;
use super::section;
use super::value::write_tagged_id;
use crate::semantic::codec::common::{write_count, write_symbol_reference};
use crate::semantic::model::{
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementKind, InterfaceDependencyRequirementValue,
    InterfaceDependencySubject, InterfaceDependencySubjectRoot,
};
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemanticFacts};
use bray_symbols::{BorrowKind, LifecycleObligationKind};

pub(super) fn encode_contracts(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    write_count(&mut encoder, facts.dependency_contracts.len());
    write_count(&mut encoder, facts.constraints.len());
    write_count(&mut encoder, facts.callable_contracts.len());

    for contract in &*facts.dependency_contracts {
        write_count(&mut encoder, contract.requirements.len());

        for requirement in &*contract.requirements {
            encode_dependency_requirement(&mut encoder, requirement);
        }
    }

    for constraint in &*facts.constraints {
        write_symbol_reference(&mut encoder, &constraint.owner);
        encoder.write_u32(constraint.ordinal.raw());
        encoder.write_u32(constraint.predicate.dependency_contract.raw());
    }

    for contract in &*facts.callable_contracts {
        write_symbol_reference(&mut encoder, &contract.owner);
        write_count(&mut encoder, contract.clauses.len());

        for clause in &*contract.clauses {
            encoder.write_u32(clause.ordinal.raw());
            encoder.write_u32(clause.kind.to_wire());
            encoder.write_u32(clause.predicate.dependency_contract.raw());
        }

        write_count(&mut encoder, contract.trusted_capabilities.len());

        for capability in &*contract.trusted_capabilities {
            write_symbol_reference(&mut encoder, capability);
        }

        encoder.write_u32(contract.dependency_contract.raw());
    }

    section(
        InterfaceSectionTag::Contracts,
        facts.dependency_contracts.len() + facts.constraints.len() + facts.callable_contracts.len(),
        encoder,
    )
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
    match subject.root {
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
            encoder.write_u32(match kind {
                LifecycleObligationKind::Destruction => 1,
                LifecycleObligationKind::Finalization => 2,
                LifecycleObligationKind::Cancellation => 3,
                LifecycleObligationKind::Joining => 4,
            });
        }
    }
}
