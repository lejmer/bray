use super::model::EncodedSemanticSection;
use super::section;
use crate::semantic::codec::common::{
    write_count, write_optional_u32, write_string, write_symbol_reference,
};
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemanticFacts};

pub(super) fn encode_implementations(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    write_count(&mut encoder, facts.implementations.len());
    write_count(&mut encoder, facts.coherence.len());

    for implementation in &*facts.implementations {
        write_symbol_reference(&mut encoder, &implementation.implementation);
        encoder.write_u32(implementation.subject.raw());

        write_optional_u32(
            &mut encoder,
            implementation
                .trait_application
                .map(|application| application.raw()),
        );
    }

    for coherence in &*facts.coherence {
        encoder.write_u32(coherence.subject.raw());
        encoder.write_u32(coherence.trait_application.raw());

        write_count(&mut encoder, coherence.implementations.len());

        for implementation in &*coherence.implementations {
            write_symbol_reference(&mut encoder, implementation);
        }
    }

    section(
        InterfaceSectionTag::Implementations,
        facts.implementations.len() + facts.coherence.len(),
        encoder,
    )
}

pub(super) fn encode_target_dependencies(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    write_count(&mut encoder, facts.target_dependencies.len());
    write_count(&mut encoder, facts.abi_dependencies.len());

    for dependency in &*facts.target_dependencies {
        write_symbol_reference(&mut encoder, &dependency.fact);
        encoder.write_u32(dependency.value.raw());
    }

    for dependency in &*facts.abi_dependencies {
        write_symbol_reference(&mut encoder, &dependency.symbol);
        encoder.write_u32(dependency.abi.to_wire());
    }

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
