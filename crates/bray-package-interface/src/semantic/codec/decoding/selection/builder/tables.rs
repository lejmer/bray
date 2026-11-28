use crate::semantic::codec::common::SemanticDecodeContext;
use crate::semantic::codec::decoding::{bundle, contract, declaration, support, surface, template, value};
use crate::semantic::model::InterfaceSemanticRecordKind;
use crate::{InterfaceSectionTag, InterfaceValidationError, ValidatedInterfaceSection};

pub(super) struct SelectedTables<'bytes> {
    pub(super) types: value::TypeRecordTables<'bytes>,
    pub(super) constants: value::ConstantRecordTables<'bytes>,
    pub(super) contracts: contract::ContractRecordTables<'bytes>,
    pub(super) implementations: Option<surface::ImplementationRecordTables<'bytes>>,
    pub(super) targets: Option<surface::TargetRecordTables<'bytes>>,
    pub(super) declarations: Option<declaration::DeclarationRecordTables<'bytes>>,
    pub(super) templates: Option<template::TemplateRecordTables<'bytes>>,
    // The support graph has no random access, so a template selection reads it whole.
    pub(super) support: Option<Vec<crate::semantic::model::InterfaceSupportEntity>>,
}

impl<'bytes> SelectedTables<'bytes> {
    pub(super) fn read(
        sections: &'bytes [ValidatedInterfaceSection<'bytes>],
        kind: InterfaceSemanticRecordKind,
        context: &mut SemanticDecodeContext,
    ) -> Result<Self, InterfaceValidationError> {
        let types = bundle::required_section(sections, InterfaceSectionTag::SemanticTypes)?;
        let constants = bundle::required_section(sections, InterfaceSectionTag::Constants)?;
        let contracts = bundle::required_section(sections, InterfaceSectionTag::Contracts)?;

        let types = value::decode_type_tables(types, context)?;
        let constants = value::decode_constant_tables(constants, context)?;
        let contracts = contract::decode_contract_tables(contracts, context)?;

        let implementations = if kind == InterfaceSemanticRecordKind::Implementation {
            let section = bundle::required_section(sections, InterfaceSectionTag::Implementations)?;

            Some(surface::decode_implementation_tables(section, context)?)
        } else {
            None
        };

        let targets = if matches!(
            kind,
            InterfaceSemanticRecordKind::Implementation
                | InterfaceSemanticRecordKind::TargetProperty
                | InterfaceSemanticRecordKind::Runtime
        ) {
            let section =
                bundle::required_section(sections, InterfaceSectionTag::TargetDependencies)?;

            Some(surface::decode_target_tables(section, context)?)
        } else {
            None
        };

        let declarations = if matches!(
            kind,
            InterfaceSemanticRecordKind::CallableSignature
                | InterfaceSemanticRecordKind::CallableContracts
                | InterfaceSemanticRecordKind::GenericDeclaration
                | InterfaceSemanticRecordKind::DeclaredType
                | InterfaceSemanticRecordKind::TypeRepresentation
        ) {
            let section =
                bundle::required_section(sections, InterfaceSectionTag::DeclarationSemantics)?;

            Some(declaration::decode_declaration_tables(section, context)?)
        } else {
            None
        };

        let (templates, support) = if kind == InterfaceSemanticRecordKind::DeclarationTemplate {
            let section = bundle::required_section(sections, InterfaceSectionTag::DeclarationTemplates)?;
            let templates = template::decode_template_tables(section, context)?;
            let section = bundle::required_section(sections, InterfaceSectionTag::SupportGraph)?;
            let support = support::decode_support_entities(section, context.limits(), context)?;

            (Some(templates), Some(support))
        } else {
            (None, None)
        };

        Ok(Self {
            types,
            constants,
            contracts,
            implementations,
            targets,
            declarations,
            templates,
            support,
        })
    }
}
