use bray_symbols::{DeclaredCopyContract, DeclaredLayoutMode};

use super::bundle::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::common::{write_count, write_symbol_reference};
use crate::semantic::codec::record::encode_record_table;
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemantics, InterfaceStorageShape};

pub(super) fn encode_declaration_semantics(
    semantics: &InterfaceSemantics,
) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encoder.write_u32(super::super::DECLARATION_SEMANTICS_FORMAT_VERSION);

    encode_record_table(
        &mut encoder,
        &semantics.callable_signatures,
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
            encoder.write_u32(u32::from(signature.has_body));
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.generic_declarations,
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
        &semantics.callable_parameter_defaults,
        |encoder, default| {
            write_symbol_reference(encoder, &default.parameter);
            encoder.write_u32(u32::from(default.is_present));
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.predicate_definitions,
        |encoder, definition| {
            write_symbol_reference(encoder, &definition.owner);
            encoder.write_u32(definition.state.to_wire());
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.declared_types,
        |encoder, declared| {
            write_symbol_reference(encoder, &declared.owner);
            encoder.write_u32(declared.ty.raw());
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.type_representations,
        |encoder, representation| {
            write_symbol_reference(encoder, &representation.owner);
            encoder.write_u32(encode_layout(representation.layout));
            write_optional_u64(encoder, representation.alignment);
            write_optional_u64(encoder, representation.packing);
            write_optional_u64(encoder, representation.opaque_size);
            encoder.write_u32(u32::from(representation.incomplete));
            encoder.write_u32(u32::from(representation.tagless_union));

            super::super::common::write_optional_u32(
                encoder,
                representation
                    .union_tag_type
                    .map(crate::InterfaceTypeId::raw),
            );

            write_count(encoder, representation.union_tags.len());

            for tag in &*representation.union_tags {
                write_symbol_reference(encoder, &tag.variant);
                super::value::encode_integer(encoder, &tag.value);
            }

            match &representation.storage {
                InterfaceStorageShape::Structure(members) => {
                    encoder.write_u32(1);
                    write_count(encoder, members.len());

                    for member in &**members {
                        write_optional_symbol_reference(encoder, member.field.as_ref());
                        encoder.write_u32(member.ty.raw());
                    }
                }
                InterfaceStorageShape::Union(variants) => {
                    encoder.write_u32(2);
                    write_count(encoder, variants.len());

                    for variant in &**variants {
                        write_symbol_reference(encoder, &variant.variant);
                        write_count(encoder, variant.members.len());

                        for member in &*variant.members {
                            write_optional_symbol_reference(encoder, member.field.as_ref());
                            encoder.write_u32(member.ty.raw());
                        }
                    }
                }
            }

            encoder.write_u32(encode_copy(representation.copy));
            write_count(encoder, representation.copy_dependencies.len());

            for dependency in &*representation.copy_dependencies {
                write_symbol_reference(encoder, dependency);
            }

            encoder.write_u32(u32::from(representation.plain_storage));
            encoder.write_u32(u32::from(representation.finite_size));
        },
    );

    section(
        InterfaceSectionTag::DeclarationSemantics,
        semantics.callable_signatures.len()
            + semantics.generic_declarations.len()
            + semantics.callable_parameter_defaults.len()
            + semantics.predicate_definitions.len()
            + semantics.declared_types.len()
            + semantics.type_representations.len(),
        encoder,
    )
}

fn write_optional_symbol_reference(
    encoder: &mut WireEncoder,
    reference: Option<&crate::InterfaceSymbolReference>,
) {
    match reference {
        Some(reference) => {
            encoder.write_u32(1);
            write_symbol_reference(encoder, reference);
        }
        None => encoder.write_u32(0),
    }
}

fn encode_layout(layout: DeclaredLayoutMode) -> u32 {
    match layout {
        DeclaredLayoutMode::Default => 1,
        DeclaredLayoutMode::Stable => 2,
        DeclaredLayoutMode::C => 3,
        DeclaredLayoutMode::Transparent => 4,
    }
}

fn encode_copy(copy: DeclaredCopyContract) -> u32 {
    match copy {
        DeclaredCopyContract::Absent => 1,
        DeclaredCopyContract::Unconditional => 2,
        DeclaredCopyContract::Conditional => 3,
    }
}

fn write_optional_u64(encoder: &mut WireEncoder, value: Option<u64>) {
    match value {
        Some(value) => {
            encoder.write_u32(1);
            encoder.write_u64(value);
        }
        None => encoder.write_u32(0),
    }
}
