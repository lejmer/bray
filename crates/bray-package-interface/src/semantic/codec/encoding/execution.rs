use bray_symbols::{CallableExecutionObligation, CallableExecutionTarget, SymbolOrdinal};

use crate::InterfaceCallableExecutionContract;
use crate::semantic::codec::common::write_count;
use crate::tag::WireTag;
use crate::wire::WireEncoder;

pub(super) fn encode_execution_contract(
    encoder: &mut WireEncoder,
    contract: &InterfaceCallableExecutionContract,
) {
    write_count(encoder, contract.domains.len());

    for domain in &*contract.domains {
        encoder.write_u32(domain.ordinal.raw());
        write_count(encoder, domain.entry.len());

        for term in &*domain.entry {
            encoder.write_u32(term.raw());
        }

        write_count(encoder, domain.properties.len());

        for property in &*domain.properties {
            encoder.write_u32(property.to_wire());
        }

        write_count(encoder, domain.postconditions.len());

        for (ordinal, term) in &*domain.postconditions {
            encoder.write_u32(ordinal.raw());
            encoder.write_u32(term.raw());
        }
    }

    write_count(encoder, contract.evidence.len());

    for proof in &*contract.evidence {
        encoder.write_u32(proof.origin.to_wire());
        encode_obligation(encoder, proof.obligation);
        write_count(encoder, proof.dependencies.len());

        for (target, obligation) in &*proof.dependencies {
            match target {
                CallableExecutionTarget::Callable(callable) => {
                    encoder.write_u32(0);
                    encoder.write_u32(callable.raw());
                }
                CallableExecutionTarget::Indirect(ty) => {
                    encoder.write_u32(1);
                    encoder.write_u32(ty.raw());
                }
            }

            encode_obligation(encoder, *obligation);
        }
    }
}

fn encode_obligation(
    encoder: &mut WireEncoder,
    obligation: CallableExecutionObligation<SymbolOrdinal>,
) {
    match obligation {
        CallableExecutionObligation::Property(property, guard) => {
            encoder.write_u32(0);
            encoder.write_u32(property.to_wire());

            match guard {
                Some(ordinal) => {
                    encoder.write_u32(1);
                    encoder.write_u32(ordinal.raw());
                }
                None => encoder.write_u32(0),
            }
        }
        CallableExecutionObligation::Postcondition(ordinal) => {
            encoder.write_u32(1);
            encoder.write_u32(ordinal.raw());
        }
    }
}
