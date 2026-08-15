use super::bundle::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::common::{write_count, write_external_key, write_optional_u32};
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemantics, InterfaceSupportEntity};

pub(super) fn encode_support_graph(semantics: &InterfaceSemantics) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    write_count(&mut encoder, semantics.support_entities.len());

    for entity in &*semantics.support_entities {
        match entity {
            InterfaceSupportEntity::CheckedTemplate(template) => {
                encoder.write_u32(1);
                encoder.write_u32(template.raw());
            }
            InterfaceSupportEntity::Declaration(declaration) => {
                encoder.write_u32(2);
                write_external_key(&mut encoder, declaration);
            }
            InterfaceSupportEntity::Implementation(implementation) => {
                encoder.write_u32(3);
                write_external_key(&mut encoder, implementation.declaration());
                encoder.write_u32(implementation.subject().raw());

                write_optional_u32(
                    &mut encoder,
                    implementation.trait_application().map(|id| id.raw()),
                );
            }
        }
    }

    section(
        InterfaceSectionTag::SupportGraph,
        semantics.support_entities.len(),
        encoder,
    )
}
