use super::facts::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::coherence::coherence_record_indexes;
use crate::semantic::codec::common::{
    write_count, write_optional_u32, write_string, write_symbol_reference,
};
use crate::semantic::codec::record::encode_record_table;
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemanticFacts};

pub(super) fn encode_implementations(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let Some(coherence_by_implementation) = coherence_record_indexes(&facts.coherence) else {
        unreachable!("validated coherence indexes fit the wire format");
    };

    let mut encoder = WireEncoder::new();

    encode_record_table(
        &mut encoder,
        &facts.implementations,
        |encoder, implementation| {
            write_symbol_reference(encoder, &implementation.implementation);
            encoder.write_u32(implementation.subject.raw());

            write_optional_u32(
                encoder,
                implementation
                    .trait_application
                    .map(|application| application.raw()),
            );

            let coherence = coherence_by_implementation
                .get(&implementation.implementation)
                .map_or(&[][..], Vec::as_slice);

            write_count(encoder, coherence.len());

            for index in coherence {
                encoder.write_u32(*index);
            }
        },
    );

    encode_record_table(&mut encoder, &facts.coherence, |encoder, coherence| {
        encoder.write_u32(coherence.subject.raw());
        encoder.write_u32(coherence.trait_application.raw());

        write_count(encoder, coherence.implementations.len());

        for implementation in &*coherence.implementations {
            write_symbol_reference(encoder, implementation);
        }
    });

    section(
        InterfaceSectionTag::Implementations,
        facts.implementations.len() + facts.coherence.len(),
        encoder,
    )
}

pub(super) fn encode_target_dependencies(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encode_record_table(
        &mut encoder,
        &facts.target_dependencies,
        |encoder, dependency| {
            write_symbol_reference(encoder, &dependency.owner);
            write_symbol_reference(encoder, &dependency.fact);
            encoder.write_u32(dependency.value.raw());
        },
    );

    encode_record_table(
        &mut encoder,
        &facts.abi_dependencies,
        |encoder, dependency| {
            write_symbol_reference(encoder, &dependency.symbol);
            encoder.write_u32(dependency.abi.to_wire());
        },
    );

    section(
        InterfaceSectionTag::TargetDependencies,
        facts.target_dependencies.len() + facts.abi_dependencies.len(),
        encoder,
    )
}

pub(super) fn encode_provenance(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    for provenance in &*facts.provenance {
        write_symbol_reference(&mut encoder, &provenance.symbol);
        write_string(&mut encoder, provenance.document.as_str());

        encoder.write_u32(provenance.start);
        encoder.write_u32(provenance.end);
    }

    section(
        InterfaceSectionTag::SourceProvenance,
        facts.provenance.len(),
        encoder,
    )
}
