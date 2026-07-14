use std::collections::BTreeSet;

use bray_bound_tree::CheckedTemplateNodeId;
use bray_symbols::InterfaceSupportEntityId;

use crate::semantic::model::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceImplementationReference, InterfaceSemanticFacts,
    InterfaceSupportEntity, InterfaceTemplateReference, InterfaceType,
};
use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits,
    semantic::validation::fact::{validate_index, validate_symbol},
};

use super::checked_index;
use super::support::validate_support_entities;

impl InterfaceSemanticFacts {
    pub(super) fn validate_template_facts(
        &self,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        validate_declaration_order(self)?;

        let template_entities = validate_support_entities(self)?;

        if template_entities.len() != self.checked_templates.len()
            || self.declaration_templates.len() != self.checked_templates.len()
        {
            return Err(InterfaceValidationError::Malformed);
        }

        for (entity_index, template_index) in template_entities {
            let template = &self.checked_templates[template_index];
            let graph_size = template
                .inputs()
                .len()
                .saturating_add(template.nodes().len())
                .saturating_add(template.temporaries().len());

            limits.check(
                InterfaceLimit::TemplateGraphSize,
                u64::try_from(graph_size).unwrap_or(u64::MAX),
            )?;

            validate_template(self, template, entity_index, symbol_count, dependency_count)?;
        }

        let mut mapped_entities = BTreeSet::new();

        for declaration in &*self.declaration_templates {
            validate_symbol(declaration.owner(), symbol_count, dependency_count)?;

            let entity_index = support_index(declaration.entity(), self.support_entities.len())?;

            let InterfaceSupportEntity::CheckedTemplate(template_id) =
                &self.support_entities[entity_index]
            else {
                return Err(InterfaceValidationError::Malformed);
            };

            let template_index =
                checked_index(template_id.to_index(), self.checked_templates.len())?;

            if self.checked_templates[template_index].kind() != declaration.kind()
                || !mapped_entities.insert(entity_index)
            {
                return Err(InterfaceValidationError::Malformed);
            }
        }

        if mapped_entities.len() != self.checked_templates.len() {
            return Err(InterfaceValidationError::Malformed);
        }

        Ok(())
    }
}

fn validate_declaration_order(
    facts: &InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    if !facts.declaration_templates.windows(2).all(|pair| {
        (pair[0].owner(), pair[0].kind(), pair[0].ordinal())
            < (pair[1].owner(), pair[1].kind(), pair[1].ordinal())
    }) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn validate_template(
    facts: &InterfaceSemanticFacts,
    template: &InterfaceCheckedTemplate,
    entity_index: usize,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    if template.nodes().is_empty()
        || !is_strictly_sorted(template.behavior().effects())
        || !is_strictly_sorted(template.behavior().capabilities())
        || !is_strictly_sorted(template.behavior().trusted_obligations())
        || !is_strictly_sorted(template.behavior().lifecycle_obligations())
        || !is_strictly_sorted(template.behavior().witnesses())
    {
        return Err(InterfaceValidationError::Malformed);
    }

    for input in template.inputs() {
        validate_index(input.ty().to_index(), facts.types.len())?;

        match input.kind() {
            InterfaceCheckedTemplateInputKind::GenericType(parameter)
            | InterfaceCheckedTemplateInputKind::GenericConstant(parameter) => {
                validate_symbol(parameter, symbol_count, dependency_count)?;
            }
            InterfaceCheckedTemplateInputKind::Receiver
            | InterfaceCheckedTemplateInputKind::Parameter(_)
            | InterfaceCheckedTemplateInputKind::PostconditionResult => {}
        }
    }

    for (node_index, node) in template.nodes().iter().enumerate() {
        validate_index(node.ty().to_index(), facts.types.len())?;
        validate_operation(
            facts,
            template,
            node,
            node_index,
            entity_index,
            symbol_count,
            dependency_count,
        )?;
    }

    for temporary in template.temporaries() {
        let initializer = checked_index(
            compact_index(temporary.initializer().raw()),
            template.nodes().len(),
        )?;

        validate_index(temporary.ty().to_index(), facts.types.len())?;
        validate_index(
            temporary.dependency_contract().to_index(),
            facts.dependency_contracts.len(),
        )?;

        if template.nodes()[initializer].ty() != temporary.ty() {
            return Err(InterfaceValidationError::Malformed);
        }
    }

    validate_index(
        compact_index(template.result().raw()),
        template.nodes().len(),
    )?;
    validate_index(
        template.behavior().dependency_contract().to_index(),
        facts.dependency_contracts.len(),
    )?;

    for symbol in template
        .behavior()
        .effects()
        .iter()
        .chain(template.behavior().capabilities())
        .chain(template.behavior().trusted_obligations())
    {
        validate_symbol(symbol, symbol_count, dependency_count)?;
    }

    for witness in template.behavior().witnesses() {
        validate_implementation_reference(
            facts,
            witness,
            entity_index,
            symbol_count,
            dependency_count,
        )?;
    }

    Ok(())
}

fn validate_operation(
    facts: &InterfaceSemanticFacts,
    template: &InterfaceCheckedTemplate,
    node: &InterfaceCheckedTemplateNode,
    node_index: usize,
    entity_index: usize,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    validate_operation_references(
        facts,
        template,
        node.operation(),
        node_index,
        entity_index,
        symbol_count,
        dependency_count,
    )?;

    validate_operation_type(facts, template, node)
}

fn validate_operation_references(
    facts: &InterfaceSemanticFacts,
    template: &InterfaceCheckedTemplate,
    operation: &InterfaceCheckedTemplateOperation,
    node_index: usize,
    entity_index: usize,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    match operation {
        InterfaceCheckedTemplateOperation::Input(input) => {
            checked_index(compact_index(input.raw()), template.inputs().len())?;
        }
        InterfaceCheckedTemplateOperation::Constant(constant) => {
            validate_index(constant.to_index(), facts.constant_terms.len())?;
        }
        InterfaceCheckedTemplateOperation::Declaration(declaration) => {
            validate_template_reference(
                facts,
                declaration,
                entity_index,
                symbol_count,
                dependency_count,
            )?;
        }
        InterfaceCheckedTemplateOperation::Call {
            callable,
            arguments,
            implementation,
        } => {
            validate_template_reference(
                facts,
                callable,
                entity_index,
                symbol_count,
                dependency_count,
            )?;
            validate_prior_nodes(arguments, node_index)?;

            if let Some(implementation) = implementation {
                validate_implementation_reference(
                    facts,
                    implementation,
                    entity_index,
                    symbol_count,
                    dependency_count,
                )?;
            }
        }
        InterfaceCheckedTemplateOperation::Convert { value, target } => {
            validate_prior_node(*value, node_index)?;
            validate_index(target.to_index(), facts.types.len())?;
        }
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            validate_prior_nodes(elements, node_index)?;
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            validate_prior_nodes(elements, node_index)?;
        }
        InterfaceCheckedTemplateOperation::Project { subject, member } => {
            validate_prior_node(*subject, node_index)?;
            validate_template_reference(
                facts,
                member,
                entity_index,
                symbol_count,
                dependency_count,
            )?;
        }
        InterfaceCheckedTemplateOperation::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            validate_prior_nodes(&[*condition, *when_true, *when_false], node_index)?;
        }
        InterfaceCheckedTemplateOperation::ShortCircuit { left, right, .. } => {
            validate_prior_nodes(&[*left, *right], node_index)?;
        }
        InterfaceCheckedTemplateOperation::Temporary(temporary) => {
            let temporary_index =
                checked_index(compact_index(temporary.raw()), template.temporaries().len())?;
            let temporary = template.temporaries()[temporary_index];

            validate_prior_node(temporary.initializer(), node_index)?;
        }
    }

    Ok(())
}

fn validate_operation_type(
    facts: &InterfaceSemanticFacts,
    template: &InterfaceCheckedTemplate,
    node: &InterfaceCheckedTemplateNode,
) -> Result<(), InterfaceValidationError> {
    let valid = match node.operation() {
        InterfaceCheckedTemplateOperation::Input(input) => {
            let input_index = checked_index(compact_index(input.raw()), template.inputs().len())?;

            template.inputs()[input_index].ty() == node.ty()
        }
        InterfaceCheckedTemplateOperation::Convert { target, .. } => *target == node.ty(),
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            let Some(InterfaceType::Tuple(types)) = type_at(facts, node.ty()) else {
                return Err(InterfaceValidationError::Malformed);
            };

            elements.len() == types.len()
                && elements
                    .iter()
                    .zip(types.iter())
                    .all(|(element, ty)| node_type(template, *element) == Some(*ty))
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            let Some(InterfaceType::Array { element, .. }) = type_at(facts, node.ty()) else {
                return Err(InterfaceValidationError::Malformed);
            };

            elements
                .iter()
                .all(|node| node_type(template, *node) == Some(*element))
        }
        InterfaceCheckedTemplateOperation::Conditional {
            when_true,
            when_false,
            ..
        } => {
            node_type(template, *when_true) == Some(node.ty())
                && node_type(template, *when_false) == Some(node.ty())
        }
        InterfaceCheckedTemplateOperation::ShortCircuit { left, right, .. } => {
            node_type(template, *left) == Some(node.ty())
                && node_type(template, *right) == Some(node.ty())
        }
        InterfaceCheckedTemplateOperation::Temporary(temporary) => {
            let temporary_index =
                checked_index(compact_index(temporary.raw()), template.temporaries().len())?;

            template.temporaries()[temporary_index].ty() == node.ty()
        }
        InterfaceCheckedTemplateOperation::Constant(_)
        | InterfaceCheckedTemplateOperation::Declaration(_)
        | InterfaceCheckedTemplateOperation::Call { .. }
        | InterfaceCheckedTemplateOperation::Project { .. } => true,
    };

    if !valid {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn validate_template_reference(
    facts: &InterfaceSemanticFacts,
    reference: &InterfaceTemplateReference,
    entity_index: usize,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    match reference {
        InterfaceTemplateReference::Symbol(symbol) => {
            validate_symbol(symbol, symbol_count, dependency_count)
        }
        InterfaceTemplateReference::Support(entity) => {
            let reference_index = support_index(*entity, facts.support_entities.len())?;

            if reference_index >= entity_index
                || !matches!(
                    facts.support_entities[reference_index],
                    InterfaceSupportEntity::Declaration(_)
                )
            {
                return Err(InterfaceValidationError::Malformed);
            }

            Ok(())
        }
    }
}

fn validate_implementation_reference(
    facts: &InterfaceSemanticFacts,
    reference: &InterfaceImplementationReference,
    entity_index: usize,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    match reference {
        InterfaceImplementationReference::Symbol(symbol) => {
            validate_symbol(symbol, symbol_count, dependency_count)
        }
        InterfaceImplementationReference::Support(entity) => {
            let reference_index = support_index(*entity, facts.support_entities.len())?;

            if reference_index >= entity_index
                || !matches!(
                    facts.support_entities[reference_index],
                    InterfaceSupportEntity::Implementation(_)
                )
            {
                return Err(InterfaceValidationError::Malformed);
            }

            Ok(())
        }
    }
}

fn validate_prior_nodes(
    nodes: &[CheckedTemplateNodeId],
    current: usize,
) -> Result<(), InterfaceValidationError> {
    for node in nodes {
        validate_prior_node(*node, current)?;
    }

    Ok(())
}

fn validate_prior_node(
    node: CheckedTemplateNodeId,
    current: usize,
) -> Result<(), InterfaceValidationError> {
    if compact_index(node.raw()).is_none_or(|index| index >= current) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn node_type(
    template: &InterfaceCheckedTemplate,
    node: CheckedTemplateNodeId,
) -> Option<crate::InterfaceTypeId> {
    template
        .nodes()
        .get(compact_index(node.raw())?)
        .map(|node| node.ty())
}

fn type_at(facts: &InterfaceSemanticFacts, ty: crate::InterfaceTypeId) -> Option<&InterfaceType> {
    facts.types.get(ty.to_index()?)
}

fn support_index(
    entity: InterfaceSupportEntityId,
    length: usize,
) -> Result<usize, InterfaceValidationError> {
    checked_index(entity.to_index(), length)
}

fn compact_index(raw: u32) -> Option<usize> {
    usize::try_from(raw).ok()
}
