use std::sync::Arc;

use bray_bound_tree::{
    CheckedTemplateBehavior, CheckedTemplateBuilder, CheckedTemplateCapability,
    CheckedTemplateCompletion, CheckedTemplateEffect, CheckedTemplateExecution,
    CheckedTemplateExecutionRequirement, CheckedTemplateInput, CheckedTemplateInputKind,
    CheckedTemplateNode, CheckedTemplateOperation, CheckedTemplateTrustedObligation,
    CheckedTemplateWitness,
};
use bray_symbols::{ExternalSymbolKey, InterfaceSupportEntityId};

use crate::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateOperation,
    InterfaceImplementationReference, InterfaceSemanticFacts, InterfaceSupportEntity,
    InterfaceTemplateReference,
};

use super::common::resolve_external_key;
use super::{
    ImportedDeclarationTemplateFact, InterfaceSemanticInternError, InterfaceSymbolResolver,
    InternState,
};

impl InternState {
    pub(super) fn convert_declaration_templates(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedDeclarationTemplateFact>, InterfaceSemanticInternError> {
        let templates = facts
            .checked_templates
            .iter()
            .map(|template| {
                self.convert_template(template, facts, symbols)
                    .map(Arc::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        facts
            .declaration_templates
            .iter()
            .map(|declaration| {
                let entity = support_entity(facts, declaration.entity())?;

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

                Ok(ImportedDeclarationTemplateFact {
                    owner,
                    kind: declaration.kind(),
                    ordinal: declaration.ordinal(),
                    template,
                })
            })
            .collect()
    }

    fn convert_template(
        &self,
        template: &InterfaceCheckedTemplate,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<bray_bound_tree::CheckedTemplate, InterfaceSemanticInternError> {
        let behavior = convert_behavior(self, template, facts, symbols)?;
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

        for node in template.nodes() {
            let operation = convert_operation(self, node.operation(), facts, symbols)?;

            let ty = self
                .type_id(node.ty())
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            builder
                .push_node(CheckedTemplateNode::new(operation, ty))
                .map_err(InterfaceSemanticInternError::InvalidTemplate)?;
        }

        for temporary in template.temporaries() {
            let ty = self
                .type_id(temporary.ty())
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            let dependency = self
                .dependency_contract_id(temporary.dependency_contract())
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            builder
                .push_temporary(temporary.initializer(), ty, dependency)
                .map_err(InterfaceSemanticInternError::InvalidTemplate)?;
        }

        builder
            .finish(template.result(), CheckedTemplateCompletion::Complete)
            .map_err(InterfaceSemanticInternError::InvalidTemplate)
    }
}

fn convert_input_kind(
    kind: &InterfaceCheckedTemplateInputKind,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateInputKind, InterfaceSemanticInternError> {
    match kind {
        InterfaceCheckedTemplateInputKind::GenericType(reference) => {
            resolve_external_key(symbols, reference).map(CheckedTemplateInputKind::GenericType)
        }
        InterfaceCheckedTemplateInputKind::GenericConstant(reference) => {
            resolve_external_key(symbols, reference).map(CheckedTemplateInputKind::GenericConstant)
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
    facts: &InterfaceSemanticFacts,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateOperation, InterfaceSemanticInternError> {
    match operation {
        InterfaceCheckedTemplateOperation::Input(input) => {
            Ok(CheckedTemplateOperation::Input(*input))
        }
        InterfaceCheckedTemplateOperation::Constant(constant) => state
            .constant_term_id(*constant)
            .map(CheckedTemplateOperation::Constant)
            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph),
        InterfaceCheckedTemplateOperation::Declaration(reference) => Ok(
            CheckedTemplateOperation::Declaration(template_key(facts, reference, symbols)?),
        ),
        InterfaceCheckedTemplateOperation::Call {
            callable,
            arguments,
            implementation,
        } => Ok(CheckedTemplateOperation::call(
            template_key(facts, callable, symbols)?,
            arguments.iter().copied(),
            implementation
                .as_ref()
                .map(|reference| implementation_key(facts, reference, symbols))
                .transpose()?,
        )),
        InterfaceCheckedTemplateOperation::Convert { value, target } => {
            let target = state
                .type_id(*target)
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            Ok(CheckedTemplateOperation::Convert {
                value: *value,
                target,
            })
        }
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            Ok(CheckedTemplateOperation::tuple(elements.iter().copied()))
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            Ok(CheckedTemplateOperation::array(elements.iter().copied()))
        }
        InterfaceCheckedTemplateOperation::Project { subject, member } => {
            Ok(CheckedTemplateOperation::Project {
                subject: *subject,
                member: template_key(facts, member, symbols)?,
            })
        }
        InterfaceCheckedTemplateOperation::Conditional {
            condition,
            when_true,
            when_false,
        } => Ok(CheckedTemplateOperation::Conditional {
            condition: *condition,
            when_true: *when_true,
            when_false: *when_false,
        }),
        InterfaceCheckedTemplateOperation::ShortCircuit { kind, left, right } => {
            Ok(CheckedTemplateOperation::ShortCircuit {
                kind: *kind,
                left: *left,
                right: *right,
            })
        }
        InterfaceCheckedTemplateOperation::Temporary(temporary) => {
            Ok(CheckedTemplateOperation::Temporary(*temporary))
        }
    }
}

fn convert_behavior(
    state: &InternState,
    template: &InterfaceCheckedTemplate,
    facts: &InterfaceSemanticFacts,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateBehavior, InterfaceSemanticInternError> {
    let behavior = template.behavior();

    let dependency_contract = state
        .dependency_contract_id(behavior.dependency_contract())
        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

    Ok(CheckedTemplateBehavior::new(
        behavior
            .effects()
            .iter()
            .map(|reference| {
                resolve_external_key(symbols, reference).map(CheckedTemplateEffect::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .capabilities()
            .iter()
            .map(|reference| {
                resolve_external_key(symbols, reference).map(CheckedTemplateCapability::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .trusted_obligations()
            .iter()
            .map(|reference| {
                resolve_external_key(symbols, reference).map(CheckedTemplateTrustedObligation::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
        CheckedTemplateExecution::new(
            behavior
                .execution_requirements()
                .iter()
                .map(|reference| {
                    resolve_external_key(symbols, reference)
                        .map(CheckedTemplateExecutionRequirement::new)
                })
                .collect::<Result<Vec<_>, _>>()?,
            behavior.current_run_cancellation(),
        ),
        behavior.lifecycle_obligations().iter().copied(),
        dependency_contract,
        behavior
            .witnesses()
            .iter()
            .map(|reference| {
                implementation_key(facts, reference, symbols).map(CheckedTemplateWitness::new)
            })
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn template_key(
    facts: &InterfaceSemanticFacts,
    reference: &InterfaceTemplateReference,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<ExternalSymbolKey, InterfaceSemanticInternError> {
    match reference {
        InterfaceTemplateReference::Symbol(reference) => resolve_external_key(symbols, reference),
        InterfaceTemplateReference::Support(entity) => support_declaration_key(facts, *entity),
    }
}

fn implementation_key(
    facts: &InterfaceSemanticFacts,
    reference: &InterfaceImplementationReference,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<ExternalSymbolKey, InterfaceSemanticInternError> {
    match reference {
        InterfaceImplementationReference::Symbol(reference) => {
            resolve_external_key(symbols, reference)
        }
        InterfaceImplementationReference::Support(entity) => {
            let entity_data = support_entity(facts, *entity)?;

            // External keys are Arc-backed and checked templates own stable keys.
            match entity_data {
                InterfaceSupportEntity::Implementation(implementation) => {
                    Ok(implementation.declaration().clone())
                }
                InterfaceSupportEntity::Declaration(key) => Ok(key.clone()),
                InterfaceSupportEntity::CheckedTemplate(_) => {
                    Err(InterfaceSemanticInternError::InvalidSupportEntity(*entity))
                }
            }
        }
    }
}

fn support_declaration_key(
    facts: &InterfaceSemanticFacts,
    entity: InterfaceSupportEntityId,
) -> Result<ExternalSymbolKey, InterfaceSemanticInternError> {
    // External keys are Arc-backed and checked templates own stable keys.
    match support_entity(facts, entity)? {
        InterfaceSupportEntity::Declaration(key) => Ok(key.clone()),
        InterfaceSupportEntity::Implementation(implementation) => {
            Ok(implementation.declaration().clone())
        }
        InterfaceSupportEntity::CheckedTemplate(_) => {
            Err(InterfaceSemanticInternError::InvalidSupportEntity(entity))
        }
    }
}

fn support_entity(
    facts: &InterfaceSemanticFacts,
    entity: InterfaceSupportEntityId,
) -> Result<&InterfaceSupportEntity, InterfaceSemanticInternError> {
    entity
        .to_index()
        .and_then(|index| facts.support_entities.get(index))
        .ok_or(InterfaceSemanticInternError::InvalidSupportEntity(entity))
}
