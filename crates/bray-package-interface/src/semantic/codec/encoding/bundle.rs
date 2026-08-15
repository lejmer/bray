use super::{
    contract, declaration, directory, model::EncodedSemanticSection, support, surface, template,
    value,
};
use crate::wire::WireEncoder;
use crate::{
    InterfaceSectionTag, InterfaceSemantics, InterfaceValidationError, InterfaceValidationLimits,
    PackageInterfaceSurface,
};

/// Encodes one validated semantic record bundle into canonical interface sections.
///
/// Returns an error when the semantics violate the interface surface or resource limits.
pub fn encode_semantics(
    semantics: &InterfaceSemantics,
    surface: &PackageInterfaceSurface,
    limits: InterfaceValidationLimits,
) -> Result<Vec<EncodedSemanticSection>, InterfaceValidationError> {
    semantics.validate(surface, limits)?;

    Ok(encode_validated_semantics(semantics))
}

pub(crate) fn encode_validated_semantics(
    semantics: &InterfaceSemantics,
) -> Vec<EncodedSemanticSection> {
    vec![
        directory::encode_semantic_directory(semantics),
        value::encode_types(semantics),
        value::encode_constants(semantics),
        contract::encode_contracts(semantics),
        declaration::encode_declaration_semantics(semantics),
        template::encode_templates(semantics),
        surface::encode_implementations(semantics),
        surface::encode_target_dependencies(semantics),
        surface::encode_provenance(semantics),
        support::encode_support_graph(semantics),
    ]
}

pub(super) fn section(
    tag: InterfaceSectionTag,
    record_count: usize,
    encoder: WireEncoder,
) -> EncodedSemanticSection {
    EncodedSemanticSection::new(tag, record_count, encoder.into_bytes())
}
