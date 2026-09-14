use std::sync::Arc;

use bray_bound_tree::{
    CheckedTemplateBehavior, CheckedTemplateBuilder, CheckedTemplateCapability,
    CheckedTemplateCompletion, CheckedTemplateEffect, CheckedTemplateExecution,
    CheckedTemplateExecutionRequirement, CheckedTemplateInput, CheckedTemplateInputKind,
    CheckedTemplateNode, CheckedTemplateOperation, CheckedTemplateTrustedObligation,
    CheckedTemplateWitness,
};
use bray_symbols::{InterfaceSupportEntityId, SymbolKey};

use crate::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateOperation,
    InterfaceCheckedTemplateTemporary, InterfaceImplementationReference, InterfaceSemantics,
    InterfaceSupportEntity, InterfaceTemplateReference,
};

use super::common::resolve_symbol_key;
use super::{
    ImportedDeclarationTemplate, InterfaceSemanticInternError, InterfaceSymbolResolver, InternState,
};

impl InternState {
    pub(super) fn convert_declaration_templates(
        &self,
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedDeclarationTemplate>, InterfaceSemanticInternError> {
        let templates = semantics
            .checked_templates
            .iter()
            .map(|template| {
                self.convert_template(template, semantics, symbols)
                    .map(Arc::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        semantics
            .declaration_templates
            .iter()
            .map(|declaration| {
                let entity = support_entity(semantics, declaration.entity())?;

                let InterfaceSupportEntity::CheckedTemplate(template) = entity else {
                    return Err(InterfaceSemanticInternError::InvalidSupportEntity(
                        declaration.entity(),
                    ));
                };

                let Some(template) = template
                    .to_index()
                    .and_then(|index| templates.get(index))
                    .cloned()
                else {
                    return Err(InterfaceSemanticInternError::InvalidSupportEntity(
                        declaration.entity(),
                    ));
                };

                // Interface references are Arc-backed and diagnostics own their stable reference.
                let owner = symbols.resolve(declaration.owner()).ok_or_else(|| {
                    InterfaceSemanticInternError::UnresolvedSymbol(declaration.owner().clone())
                })?;

                if !declaration.kind().accepts_owner(owner.kind()) {
                    return Err(InterfaceSemanticInternError::InvalidSymbolKind(
                        declaration.owner().clone(),
                    ));
                }

                Ok(ImportedDeclarationTemplate {
                    owner,
                    kind: declaration.kind(),
                    ordinal: declaration.ordinal(),
                    entity: declaration.entity(),
                    template,
                })
            })
            .collect()
    }

    pub(super) fn convert_template(
        &self,
        template: &InterfaceCheckedTemplate,
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<bray_bound_tree::CheckedTemplate, InterfaceSemanticInternError> {
        let behavior = convert_behavior(self, template, semantics, symbols)?;
        let mut builder = CheckedTemplateBuilder::new(template.kind(), behavior);

        for input in template.inputs() {
            let kind = convert_input_kind(input.kind(), symbols)?;

            let ty = self
                .type_id(input.ty())
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            builder
                .push_input(CheckedTemplateInput::new(kind, ty))
                .map_err(InterfaceSemanticInternError::InvalidTemplate)?;
        }

        let mut temporaries = template.temporaries().iter().peekable();

        for (node_index, node) in template.nodes().iter().enumerate() {
            while temporaries.peek().is_some_and(|temporary| {
                usize::try_from(temporary.initializer().raw())
                    .is_ok_and(|initializer| initializer < node_index)
            }) {
                let Some(temporary) = temporaries.next() else {
                    break;
                };

                push_temporary(self, &mut builder, temporary)?;
            }

            let operation = convert_operation(self, node.operation(), semantics, symbols)?;

            let ty = self
                .type_id(node.ty())
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            builder
                .push_node(CheckedTemplateNode::new(operation, ty))
                .map_err(InterfaceSemanticInternError::InvalidTemplate)?;
        }

        for temporary in temporaries {
            push_temporary(self, &mut builder, temporary)?;
        }

        builder
            .finish(template.result(), CheckedTemplateCompletion::Complete)
            .map_err(InterfaceSemanticInternError::InvalidTemplate)
    }
}

fn push_temporary(
    state: &InternState,
    builder: &mut CheckedTemplateBuilder,
    temporary: &InterfaceCheckedTemplateTemporary,
) -> Result<(), InterfaceSemanticInternError> {
    let ty = state
        .type_id(temporary.ty())
        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

    let dependency = state
        .dependency_contract_id(temporary.dependency_contract())
        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

    builder
        .push_temporary(temporary.initializer(), ty, dependency)
        .map_err(InterfaceSemanticInternError::InvalidTemplate)?;

    Ok(())
}

fn convert_input_kind(
    kind: &InterfaceCheckedTemplateInputKind,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateInputKind, InterfaceSemanticInternError> {
    match kind {
        InterfaceCheckedTemplateInputKind::GenericType(reference) => {
            resolve_symbol_key(symbols, reference).map(CheckedTemplateInputKind::GenericType)
        }
        InterfaceCheckedTemplateInputKind::GenericConstant(reference) => {
            resolve_symbol_key(symbols, reference).map(CheckedTemplateInputKind::GenericConstant)
        }
        InterfaceCheckedTemplateInputKind::Receiver => Ok(CheckedTemplateInputKind::Receiver),
        InterfaceCheckedTemplateInputKind::Parameter(ordinal) => {
            Ok(CheckedTemplateInputKind::Parameter(*ordinal))
        }
        InterfaceCheckedTemplateInputKind::PostconditionResult => {
            Ok(CheckedTemplateInputKind::PostconditionResult)
        }
    }
}

fn convert_operation(
    state: &InternState,
    operation: &InterfaceCheckedTemplateOperation,
    semantics: &InterfaceSemantics,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateOperation, InterfaceSemanticInternError> {
    operation.try_map_references(
        |term| {
            state
                .constant_term_id(*term)
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)
        },
        |ty| {
            state
                .type_id(*ty)
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)
        },
        |reference| template_key(semantics, reference, symbols),
        |substitution| {
            state
                .substitution_id(*substitution)
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)
        },
        |reference| implementation_key(semantics, reference, symbols),
    )
}

fn convert_behavior(
    state: &InternState,
    template: &InterfaceCheckedTemplate,
    semantics: &InterfaceSemantics,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateBehavior, InterfaceSemanticInternError> {
    let behavior = template.behavior();

    let dependency_contract = state
        .dependency_contract_id(behavior.dependency_contract())
        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

    let result_dependencies = state
        .dependency_contract_id(behavior.result_dependencies())
        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

    Ok(CheckedTemplateBehavior::new(
        behavior
            .effects()
            .iter()
            .map(|reference| resolve_symbol_key(symbols, reference).map(CheckedTemplateEffect::new))
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .capabilities()
            .iter()
            .map(|reference| {
                resolve_symbol_key(symbols, reference).map(CheckedTemplateCapability::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .trusted_obligations()
            .iter()
            .map(|reference| {
                resolve_symbol_key(symbols, reference).map(CheckedTemplateTrustedObligation::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
        CheckedTemplateExecution::new(
            behavior
                .execution_requirements()
                .iter()
                .map(|reference| {
                    resolve_symbol_key(symbols, reference)
                        .map(CheckedTemplateExecutionRequirement::new)
                })
                .collect::<Result<Vec<_>, _>>()?,
            behavior.current_run_cancellation(),
        ),
        behavior.lifecycle_obligations().iter().copied(),
        dependency_contract,
        result_dependencies,
        behavior
            .witnesses()
            .iter()
            .map(|reference| {
                implementation_key(semantics, reference, symbols).map(CheckedTemplateWitness::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn template_key(
    semantics: &InterfaceSemantics,
    reference: &InterfaceTemplateReference,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    match reference {
        InterfaceTemplateReference::Symbol(reference) => resolve_symbol_key(symbols, reference),
        InterfaceTemplateReference::Support(entity) => support_declaration_key(semantics, *entity),
    }
}

fn implementation_key(
    semantics: &InterfaceSemantics,
    reference: &InterfaceImplementationReference,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    match reference {
        InterfaceImplementationReference::Symbol(reference) => {
            resolve_symbol_key(symbols, reference)
        }
        InterfaceImplementationReference::Support(entity) => {
            let entity_data = support_entity(semantics, *entity)?;

            // External keys are Arc-backed and checked templates own stable keys.
            match entity_data {
                InterfaceSupportEntity::Implementation(implementation) => {
                    Ok(implementation.declaration().clone().into())
                }
                InterfaceSupportEntity::Declaration(key) => Ok(key.clone().into()),
                InterfaceSupportEntity::CheckedTemplate(_) => {
                    Err(InterfaceSemanticInternError::InvalidSupportEntity(*entity))
                }
            }
        }
    }
}

fn support_declaration_key(
    semantics: &InterfaceSemantics,
    entity: InterfaceSupportEntityId,
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    // External keys are Arc-backed and checked templates own stable keys.
    match support_entity(semantics, entity)? {
        InterfaceSupportEntity::Declaration(key) => Ok(key.clone().into()),
        InterfaceSupportEntity::Implementation(implementation) => {
            Ok(implementation.declaration().clone().into())
        }
        InterfaceSupportEntity::CheckedTemplate(_) => {
            Err(InterfaceSemanticInternError::InvalidSupportEntity(entity))
        }
    }
}

fn support_entity(
    semantics: &InterfaceSemantics,
    entity: InterfaceSupportEntityId,
) -> Result<&InterfaceSupportEntity, InterfaceSemanticInternError> {
    entity
        .to_index()
        .and_then(|index| semantics.support_entities.get(index))
        .ok_or(InterfaceSemanticInternError::InvalidSupportEntity(entity))
}
