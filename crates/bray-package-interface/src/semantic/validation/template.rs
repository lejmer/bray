use std::collections::BTreeSet;

use bray_bound_tree::CheckedTemplateNodeId;
use bray_symbols::{InterfaceSupportEntityId, SymbolKind};

use crate::semantic::model::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceImplementationReference, InterfaceSemantics,
    InterfaceSupportEntity, InterfaceTemplateReference, InterfaceType,
};
use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, PackageInterfaceSurface,
    semantic::validation::surface::{validate_index, validate_symbol, validate_symbol_kind},
};

use super::checked_index;
use super::declaration::validate_predicate_templates;
use super::support::validate_support_entities;

#[derive(Clone, Copy)]
struct TemplateValidationContext<'semantics> {
    semantics: &'semantics InterfaceSemantics,
    template: &'semantics InterfaceCheckedTemplate,
    entity_index: usize,
    surface: &'semantics PackageInterfaceSurface,
    symbol_count: usize,
    dependency_count: usize,
}

impl InterfaceSemantics {
    pub(super) fn validate_template_semantics(
        &self,
        surface: &PackageInterfaceSurface,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        validate_declaration_order(self)?;

        let template_entities = validate_support_entities(self, surface)?;
        let symbol_count = surface.symbols().symbols().len();
        let dependency_count = surface.dependencies().len();

        if template_entities.len() != self.checked_templates.len()
            || self.declaration_templates.len() != self.checked_templates.len()
        {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Template,
            ));
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

            validate_template(
                self,
                template,
                entity_index,
                surface,
                symbol_count,
                dependency_count,
            )?;
        }

        let mut mapped_entities = BTreeSet::new();

        for declaration in &*self.declaration_templates {
            validate_symbol(declaration.owner(), symbol_count, dependency_count)?;

            if !declaration
                .kind()
                .accepts_owner(validate_symbol_kind(declaration.owner(), surface)?)
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            }

            let entity_index = support_index(declaration.entity(), self.support_entities.len())?;

            let InterfaceSupportEntity::CheckedTemplate(template_id) =
                &self.support_entities[entity_index]
            else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            };

            let template_index =
                checked_index(template_id.to_index(), self.checked_templates.len())?;

            if self.checked_templates[template_index].kind() != declaration.kind()
                || !mapped_entities.insert(entity_index)
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            }
        }

        if mapped_entities.len() != self.checked_templates.len() {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Template,
            ));
        }

        validate_predicate_templates(self)?;

        Ok(())
    }

    pub(crate) fn validate_implementation_template(
        &self,
        surface: &PackageInterfaceSurface,
        template: &InterfaceCheckedTemplate,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        let graph_size = template
            .inputs()
            .len()
            .saturating_add(template.nodes().len())
            .saturating_add(template.temporaries().len());

        limits.check(
            InterfaceLimit::TemplateGraphSize,
            u64::try_from(graph_size).unwrap_or(u64::MAX),
        )?;

        validate_template(
            self,
            template,
            self.support_entities.len(),
            surface,
            surface.symbols().symbols().len(),
            surface.dependencies().len(),
        )
    }
}

pub(crate) fn validate_constraint_templates(
    semantics: &InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let constraints = semantics.constraints.iter().filter_map(|constraint| {
        matches!(
            constraint.kind,
            crate::InterfaceConstraintKind::Predicate(_)
        )
        .then_some((&constraint.owner, constraint.ordinal))
    });

    let templates = semantics
        .declaration_templates
        .iter()
        .filter(|template| {
            template.kind() == bray_bound_tree::CheckedTemplateKind::GenericConstraint
        })
        .map(|template| (template.owner(), template.ordinal()));

    if !constraints.eq(templates) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Template,
        ));
    }

    Ok(())
}

fn validate_declaration_order(
    semantics: &InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    if !semantics.declaration_templates.windows(2).all(|pair| {
        (pair[0].owner(), pair[0].kind(), pair[0].ordinal())
            < (pair[1].owner(), pair[1].kind(), pair[1].ordinal())
    }) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Template,
        ));
    }

    Ok(())
}

fn validate_template(
    semantics: &InterfaceSemantics,
    template: &InterfaceCheckedTemplate,
    entity_index: usize,
    surface: &PackageInterfaceSurface,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    if template.nodes().is_empty()
        || !is_strictly_sorted(template.behavior().effects())
        || !is_strictly_sorted(template.behavior().capabilities())
        || !is_strictly_sorted(template.behavior().trusted_obligations())
        || !is_strictly_sorted(template.behavior().execution_requirements())
        || !is_strictly_sorted(template.behavior().lifecycle_obligations())
        || !is_strictly_sorted(template.behavior().witnesses())
    {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Template,
        ));
    }

    let mut input_kinds = BTreeSet::new();

    for input in template.inputs() {
        if !input_kinds.insert(input.kind()) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Template,
            ));
        }

        validate_index(input.ty().to_index(), semantics.types.len())?;

        match input.kind() {
            InterfaceCheckedTemplateInputKind::GenericType(parameter) => {
                validate_symbol(parameter, symbol_count, dependency_count)?;

                if validate_symbol_kind(parameter, surface)? != SymbolKind::GenericTypeParameter {
                    return Err(crate::semantic::codec::invalid_value(
                        crate::InterfaceValidationField::Template,
                    ));
                }
            }
            InterfaceCheckedTemplateInputKind::GenericConstant(parameter) => {
                validate_symbol(parameter, symbol_count, dependency_count)?;

                if validate_symbol_kind(parameter, surface)? != SymbolKind::GenericConstParameter {
                    return Err(crate::semantic::codec::invalid_value(
                        crate::InterfaceValidationField::Template,
                    ));
                }
            }
            InterfaceCheckedTemplateInputKind::Receiver
            | InterfaceCheckedTemplateInputKind::Parameter(_)
            | InterfaceCheckedTemplateInputKind::PostconditionResult => {}
        }
    }

    let context = TemplateValidationContext {
        semantics,
        template,
        entity_index,
        surface,
        symbol_count,
        dependency_count,
    };

    for (node_index, node) in template.nodes().iter().enumerate() {
        validate_index(node.ty().to_index(), semantics.types.len())?;
        validate_operation(context, node, node_index)?;
    }

    let mut previous_initializer = None;

    for temporary in template.temporaries() {
        let initializer = checked_index(
            compact_index(temporary.initializer().raw()),
            template.nodes().len(),
        )?;

        if previous_initializer.is_some_and(|previous| initializer < previous) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Template,
            ));
        }

        previous_initializer = Some(initializer);

        validate_index(temporary.ty().to_index(), semantics.types.len())?;

        validate_index(
            temporary.dependency_contract().to_index(),
            semantics.dependency_contracts.len(),
        )?;

        if template.nodes()[initializer].ty() != temporary.ty() {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Template,
            ));
        }
    }

    validate_index(
        compact_index(template.result().raw()),
        template.nodes().len(),
    )?;

    validate_index(
        template.behavior().dependency_contract().to_index(),
        semantics.dependency_contracts.len(),
    )?;

    validate_index(
        template.behavior().result_dependencies().to_index(),
        semantics.dependency_contracts.len(),
    )?;

    for symbol in template
        .behavior()
        .effects()
        .iter()
        .chain(template.behavior().capabilities())
        .chain(template.behavior().trusted_obligations())
        .chain(template.behavior().execution_requirements())
    {
        validate_symbol(symbol, symbol_count, dependency_count)?;
    }

    for witness in template.behavior().witnesses() {
        validate_implementation_reference(context, witness)?;
    }

    Ok(())
}

fn validate_operation(
    context: TemplateValidationContext<'_>,
    node: &InterfaceCheckedTemplateNode,
    node_index: usize,
) -> Result<(), InterfaceValidationError> {
    validate_operation_references(context, node.operation(), node_index)?;

    validate_operation_type(context.semantics, context.template, node)
}

fn validate_operation_references(
    context: TemplateValidationContext<'_>,
    operation: &InterfaceCheckedTemplateOperation,
    node_index: usize,
) -> Result<(), InterfaceValidationError> {
    match operation {
        InterfaceCheckedTemplateOperation::Input(input) => {
            checked_index(compact_index(input.raw()), context.template.inputs().len())?;
        }
        InterfaceCheckedTemplateOperation::Constant { term, .. } => {
            validate_index(term.to_index(), context.semantics.constant_terms.len())?;
        }
        InterfaceCheckedTemplateOperation::Unary { operand, .. } => {
            validate_prior_node(*operand, node_index)?;
        }
        InterfaceCheckedTemplateOperation::Binary { left, right, .. } => {
            validate_prior_nodes(&[*left, *right], node_index)?;
        }
        InterfaceCheckedTemplateOperation::Borrow { operand, .. } => {
            validate_prior_node(*operand, node_index)?;
        }
        InterfaceCheckedTemplateOperation::Declaration {
            declaration,
            substitution,
        } => {
            validate_template_reference(context, declaration)?;

            if let Some(substitution) = substitution {
                validate_index(
                    substitution.to_index(),
                    context.semantics.substitutions.len(),
                )?;
            }
        }
        InterfaceCheckedTemplateOperation::Call {
            callable,
            substitution,
            arguments,
            implementation,
        } => {
            let declaration_kind = validate_template_reference(context, callable)?;

            if !declaration_kind.is_callable()
                && !matches!(
                    declaration_kind,
                    SymbolKind::Predicate
                        | SymbolKind::TraitPredicateMember
                        | SymbolKind::TraitPredicateFulfillment
                )
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            }

            validate_index(
                substitution.to_index(),
                context.semantics.substitutions.len(),
            )?;

            validate_prior_nodes(arguments, node_index)?;

            if let Some((implementation, substitution)) = implementation {
                validate_implementation_reference(context, implementation)?;

                validate_index(
                    substitution.to_index(),
                    context.semantics.substitutions.len(),
                )?;
            }
        }
        InterfaceCheckedTemplateOperation::Convert { value, target } => {
            validate_prior_node(*value, node_index)?;
            validate_index(target.to_index(), context.semantics.types.len())?;
        }
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            validate_prior_nodes(elements, node_index)?;
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            validate_prior_nodes(elements, node_index)?;
        }
        InterfaceCheckedTemplateOperation::Project { subject, member } => {
            validate_prior_node(*subject, node_index)?;
            validate_template_reference(context, member)?;
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
            let temporary_index = checked_index(
                compact_index(temporary.raw()),
                context.template.temporaries().len(),
            )?;

            let temporary = context.template.temporaries()[temporary_index];

            validate_prior_node(temporary.initializer(), node_index)?;
        }
    }

    Ok(())
}

fn validate_operation_type(
    semantics: &InterfaceSemantics,
    template: &InterfaceCheckedTemplate,
    node: &InterfaceCheckedTemplateNode,
) -> Result<(), InterfaceValidationError> {
    let valid = match node.operation() {
        InterfaceCheckedTemplateOperation::Input(input) => {
            let input_index = checked_index(compact_index(input.raw()), template.inputs().len())?;

            template.inputs()[input_index].ty() == node.ty()
        }
        InterfaceCheckedTemplateOperation::Unary { operand, .. } => {
            node_type(template, *operand) == Some(node.ty())
        }
        InterfaceCheckedTemplateOperation::Binary { .. } => true,
        InterfaceCheckedTemplateOperation::Borrow { kind, operand } => {
            let Some(InterfaceType::Borrow {
                kind: ty_kind,
                target,
            }) = type_at(semantics, node.ty())
            else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            };

            ty_kind == kind && node_type(template, *operand) == Some(*target)
        }
        InterfaceCheckedTemplateOperation::Convert { target, .. } => *target == node.ty(),
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            let Some(InterfaceType::Tuple(types)) = type_at(semantics, node.ty()) else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            };

            elements.len() == types.len()
                && elements
                    .iter()
                    .zip(types.iter())
                    .all(|(element, ty)| node_type(template, *element) == Some(*ty))
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            let Some(InterfaceType::Array { element, .. }) = type_at(semantics, node.ty()) else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
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
        InterfaceCheckedTemplateOperation::Constant { .. }
        | InterfaceCheckedTemplateOperation::Declaration { .. }
        | InterfaceCheckedTemplateOperation::Call { .. }
        | InterfaceCheckedTemplateOperation::Project { .. } => true,
    };

    if !valid {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Template,
        ));
    }

    Ok(())
}

fn validate_template_reference(
    context: TemplateValidationContext<'_>,
    reference: &InterfaceTemplateReference,
) -> Result<SymbolKind, InterfaceValidationError> {
    match reference {
        InterfaceTemplateReference::Symbol(symbol) => {
            validate_symbol(symbol, context.symbol_count, context.dependency_count)?;

            validate_symbol_kind(symbol, context.surface)
        }
        InterfaceTemplateReference::Support(entity) => {
            let reference_index = support_index(*entity, context.semantics.support_entities.len())?;

            if reference_index >= context.entity_index {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            }

            let InterfaceSupportEntity::Declaration(declaration) =
                &context.semantics.support_entities[reference_index]
            else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            };

            Ok(declaration.kind())
        }
    }
}

fn validate_implementation_reference(
    context: TemplateValidationContext<'_>,
    reference: &InterfaceImplementationReference,
) -> Result<(), InterfaceValidationError> {
    match reference {
        InterfaceImplementationReference::Symbol(symbol) => {
            validate_symbol(symbol, context.symbol_count, context.dependency_count)?;

            if !validate_symbol_kind(symbol, context.surface)?.is_implementation() {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
            }

            Ok(())
        }
        InterfaceImplementationReference::Support(entity) => {
            let reference_index = support_index(*entity, context.semantics.support_entities.len())?;

            if reference_index >= context.entity_index
                || !matches!(
                    context.semantics.support_entities[reference_index],
                    InterfaceSupportEntity::Implementation(_)
                )
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Template,
                ));
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
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Template,
        ));
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

fn type_at(semantics: &InterfaceSemantics, ty: crate::InterfaceTypeId) -> Option<&InterfaceType> {
    semantics.types.get(ty.to_index()?)
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
