use super::facts::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::common::{write_count, write_symbol_reference};
use crate::semantic::codec::record::encode_record_table;
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemanticFacts};

pub(super) fn encode_declaration_facts(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encoder.write_u32(super::super::DECLARATION_FACT_FORMAT_VERSION);

    encode_record_table(
        &mut encoder,
        &facts.callable_signatures,
        |encoder, signature| {
            write_symbol_reference(encoder, &signature.owner);
            encoder.write_u32(signature.callable_type.raw());

            match &signature.receiver {
                Some(receiver) => {
                    encoder.write_u32(1);
                    write_symbol_reference(encoder, &receiver.parameter);
                    encoder.write_u32(receiver.ty.raw());
                    encoder.write_u32(receiver.mode.to_wire());
                }
                None => encoder.write_u32(0),
            }

            write_count(encoder, signature.parameters.len());

            for parameter in &*signature.parameters {
                write_symbol_reference(encoder, parameter);
            }

            encoder.write_u32(signature.result.raw());
        },
    );

    encode_record_table(
        &mut encoder,
        &facts.generic_declarations,
        |encoder, declaration| {
            write_symbol_reference(encoder, &declaration.owner);
            write_count(encoder, declaration.parameters.len());

            for parameter in &*declaration.parameters {
                write_symbol_reference(encoder, parameter);
            }
        },
    );

    encode_record_table(
        &mut encoder,
        &facts.callable_parameter_defaults,
        |encoder, default| {
            write_symbol_reference(encoder, &default.parameter);
            encoder.write_u32(u32::from(default.is_present));
        },
    );

    encode_record_table(
        &mut encoder,
        &facts.predicate_definitions,
        |encoder, definition| {
            write_symbol_reference(encoder, &definition.owner);
            encoder.write_u32(definition.state.to_wire());
        },
    );

    section(
        InterfaceSectionTag::DeclarationFacts,
        facts.callable_signatures.len()
            + facts.generic_declarations.len()
            + facts.callable_parameter_defaults.len()
            + facts.predicate_definitions.len(),
        encoder,
    )
}
