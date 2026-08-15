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
    InterfaceCheckedTemplateTemporary, InterfaceImplementationReference, InterfaceSemanticFacts,
    InterfaceSupportEntity, InterfaceTemplateReference,
};

use super::common::resolve_symbol_key;
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
                    entity: declaration.entity(),
                    template,
                })
            })
            .collect()
    }

    pub(super) fn convert_template(
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

        let mut temporaries = template.temporaries().iter().peekable();

        for (node_index, node) in template.nodes().iter().enumerate() {
            while temporaries
                .peek()
                .is_some_and(|temporary| {
                    usize::try_from(temporary.initializer().raw())
                        .is_ok_and(|initializer| initializer < node_index)
                })
            {
                let Some(temporary) = temporaries.next() else {
                    break;
                };

                push_temporary(self, &mut builder, temporary)?;
            }

            let operation = convert_operation(self, node.operation(), facts, symbols)?;

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
    facts: &InterfaceSemanticFacts,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<CheckedTemplateOperation, InterfaceSemanticInternError> {
    match operation {
        InterfaceCheckedTemplateOperation::Input(input) => {
            Ok(CheckedTemplateOperation::Input(*input))
        }
        InterfaceCheckedTemplateOperation::Constant { term, usage } => state
            .constant_term_id(*term)
            .map(|term| CheckedTemplateOperation::Constant {
                term,
                usage: *usage,
            })
            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph),
        InterfaceCheckedTemplateOperation::Unary { operation, operand } => {
            Ok(CheckedTemplateOperation::Unary {
                operation: *operation,
                operand: *operand,
            })
        }
        InterfaceCheckedTemplateOperation::Binary {
            operation,
            left,
            right,
        } => Ok(CheckedTemplateOperation::Binary {
            operation: *operation,
            left: *left,
            right: *right,
        }),
        InterfaceCheckedTemplateOperation::Borrow { kind, operand } => {
            Ok(CheckedTemplateOperation::Borrow {
                kind: *kind,
                operand: *operand,
            })
        }
        InterfaceCheckedTemplateOperation::Declaration(reference) => Ok(
            CheckedTemplateOperation::Declaration(template_key(facts, reference, symbols)?),
        ),
        InterfaceCheckedTemplateOperation::Call {
            callable,
            substitution,
            arguments,
            implementation,
        } => {
            let substitution = state
                .substitution_id(*substitution)
                .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

            let implementation = implementation
                .as_ref()
                .map(
                    |(reference, substitution)| -> Result<_, InterfaceSemanticInternError> {
                        let declaration = implementation_key(facts, reference, symbols)?;

                        let substitution = state
                            .substitution_id(*substitution)
                            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                        Ok((declaration, substitution))
                    },
                )
                .transpose()?;

            Ok(CheckedTemplateOperation::call(
                template_key(facts, callable, symbols)?,
                substitution,
                arguments.iter().copied(),
                implementation,
            ))
        }
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
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    match reference {
        InterfaceTemplateReference::Symbol(reference) => resolve_symbol_key(symbols, reference),
        InterfaceTemplateReference::Support(entity) => support_declaration_key(facts, *entity),
    }
}

fn implementation_key(
    facts: &InterfaceSemanticFacts,
    reference: &InterfaceImplementationReference,
    symbols: &impl InterfaceSymbolResolver,
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    match reference {
        InterfaceImplementationReference::Symbol(reference) => {
            resolve_symbol_key(symbols, reference)
        }
        InterfaceImplementationReference::Support(entity) => {
            let entity_data = support_entity(facts, *entity)?;

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
    facts: &InterfaceSemanticFacts,
    entity: InterfaceSupportEntityId,
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    // External keys are Arc-backed and checked templates own stable keys.
    match support_entity(facts, entity)? {
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
    facts: &InterfaceSemanticFacts,
    entity: InterfaceSupportEntityId,
) -> Result<&InterfaceSupportEntity, InterfaceSemanticInternError> {
    entity
        .to_index()
        .and_then(|index| facts.support_entities.get(index))
        .ok_or(InterfaceSemanticInternError::InvalidSupportEntity(entity))
}
