use super::facts::section;
use super::model::EncodedSemanticSection;
use bray_runtime_interface::{ProtectedFrameAbiOperation, RuntimeAbiVersion};
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

    encode_record_table(
        &mut encoder,
        &facts.runtime_requirements,
        |encoder, requirement| {
            write_symbol_reference(encoder, requirement.owner());
            write_optional_frame(encoder, requirement.frame());

            let requirements = requirement.requirements();

            write_version(encoder, requirements.abi_version());

            match requirements.frame_abi() {
                Some(frame_abi) => {
                    encoder.write_u32(1);

                    for operation in ProtectedFrameAbiOperation::ALL {
                        write_version(encoder, frame_abi.operation(operation));
                    }
                }
                None => encoder.write_u32(0),
            }

            write_string(encoder, requirements.target().as_str());
            write_string(encoder, requirements.panic_abi().as_str());

            write_tags(encoder, requirements.capabilities());
            write_tags(encoder, requirements.lanes());
        },
    );

    section(
        InterfaceSectionTag::TargetDependencies,
        facts.target_dependencies.len()
            + facts.abi_dependencies.len()
            + facts.runtime_requirements.len(),
        encoder,
    )
}

fn write_optional_frame(
    encoder: &mut WireEncoder,
    frame: Option<bray_runtime_interface::ProtectedAsyncFrameId>,
) {
    match frame {
        Some(frame) => {
            encoder.write_u32(1);
            encoder.write_bytes(&frame.digest());
        }
        None => encoder.write_u32(0),
    }
}

fn write_version(encoder: &mut WireEncoder, version: RuntimeAbiVersion) {
    encoder.write_u32(u32::from(version.major()));
    encoder.write_u32(u32::from(version.minor()));
}

fn write_tags<T: Copy + WireTag>(encoder: &mut WireEncoder, values: &[T]) {
    write_count(encoder, values.len());

    for value in values {
        encoder.write_u32(value.to_wire());
    }
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
