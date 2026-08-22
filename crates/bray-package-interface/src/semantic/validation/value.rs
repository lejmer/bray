use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceLimit, InterfaceSemantics, InterfaceTypeId, InterfaceValidationError,
    InterfaceValidationLimits,
};

use super::saturating_u64;
use super::surface::{validate_index, validate_symbol};
use crate::semantic::model::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantValueKind,
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceType,
};

impl InterfaceSemantics {
    pub(super) fn validate_value_graph(
        &self,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        for substitution in &*self.substitutions {
            validate_symbol(&substitution.owner, symbol_count, dependency_count)?;

            for binding in &*substitution.bindings {
                validate_symbol(&binding.parameter, symbol_count, dependency_count)?;

                match binding.argument {
                    InterfaceGenericArgument::Type(id) => {
                        validate_index(id.to_index(), self.types.len())?
                    }
                    InterfaceGenericArgument::Constant(id) => {
                        validate_index(id.to_index(), self.constant_terms.len())?;
                    }
                }
            }
        }

        for application in &*self.trait_applications {
            validate_symbol(&application.definition, symbol_count, dependency_count)?;

            validate_index(
                application.substitution.to_index(),
                self.substitutions.len(),
            )?;
        }

        for instance in &*self.callable_instances {
            validate_symbol(&instance.definition, symbol_count, dependency_count)?;
            validate_index(instance.substitution.to_index(), self.substitutions.len())?;
        }

        for instance in &*self.implementation_instances {
            validate_symbol(&instance.definition, symbol_count, dependency_count)?;
            validate_index(instance.substitution.to_index(), self.substitutions.len())?;
        }

        for ty in &*self.types {
            self.validate_type(ty, symbol_count, dependency_count, limits)?;
        }

        validate_type_depth(self, limits)?;

        for value in &*self.constant_values {
            validate_index(value.ty.to_index(), self.types.len())?;
            self.validate_constant_value(&value.kind, symbol_count, dependency_count, limits)?;
        }

        for term in &*self.constant_terms {
            self.validate_constant_term(term, symbol_count, dependency_count, limits)?;
        }

        for contract in &*self.dependency_contracts {
            if !is_strictly_sorted(&contract.requirements) {
                return Err(InterfaceValidationError::Malformed);
            }

            for requirement in &*contract.requirements {
                self.validate_dependency_requirement(
                    requirement,
                    symbol_count,
                    dependency_count,
                    0,
                    limits,
                )?;
            }
        }

        Ok(())
    }

    fn validate_type(
        &self,
        ty: &InterfaceType,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        match ty {
            InterfaceType::Named {
                definition,
                substitution,
            } => {
                validate_symbol(definition, symbol_count, dependency_count)?;
                validate_index(substitution.to_index(), self.substitutions.len())?;
            }
            InterfaceType::TypeParameter(parameter) => {
                validate_symbol(parameter, symbol_count, dependency_count)?;
            }
            InterfaceType::ContextualSelf(context) => {
                validate_symbol(context, symbol_count, dependency_count)?;
            }
            InterfaceType::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                validate_index(subject.to_index(), self.types.len())?;
                validate_index(application.to_index(), self.trait_applications.len())?;
                validate_symbol(member, symbol_count, dependency_count)?;
            }
            InterfaceType::Tuple(elements) => {
                for id in &**elements {
                    validate_index(id.to_index(), self.types.len())?;
                }
            }
            InterfaceType::Array { element, length } => {
                validate_index(element.to_index(), self.types.len())?;
                validate_index(length.to_index(), self.constant_terms.len())?;
            }
            InterfaceType::FlexibleArray(id)
            | InterfaceType::Slice(id)
            | InterfaceType::Generator(id)
            | InterfaceType::Nullable(id) => {
                validate_index(id.to_index(), self.types.len())?;
            }
            InterfaceType::Borrow { target, .. } => {
                validate_index(target.to_index(), self.types.len())?;
            }
            InterfaceType::TraitView(application) => {
                validate_index(application.to_index(), self.trait_applications.len())?;
            }
            InterfaceType::OwnedIndirection { storage, target } => {
                validate_index(storage.to_index(), self.types.len())?;
                validate_index(target.to_index(), self.types.len())?;
            }
            InterfaceType::Callable {
                parameters,
                result,
                invocation_behavior,
                deferred_execution_behavior,
                ..
            } => {
                for parameter in &**parameters {
                    limits.check(
                        InterfaceLimit::StringLength,
                        saturating_u64(parameter.name.len()),
                    )?;

                    if parameter.name.is_empty() {
                        return Err(InterfaceValidationError::Malformed);
                    }

                    validate_index(parameter.ty.to_index(), self.types.len())?;
                }

                validate_index(result.to_index(), self.types.len())?;

                self.validate_callable_behavior(
                    invocation_behavior,
                    symbol_count,
                    dependency_count,
                )?;

                if let Some(behavior) = deferred_execution_behavior {
                    self.validate_callable_behavior(behavior, symbol_count, dependency_count)?;
                }
            }
        }

        Ok(())
    }

    fn validate_constant_value(
        &self,
        kind: &InterfaceConstantValueKind,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        match kind {
            InterfaceConstantValueKind::NullablePresent(id) => {
                validate_index(id.to_index(), self.constant_values.len())?;
            }
            InterfaceConstantValueKind::Tuple(values)
            | InterfaceConstantValueKind::Array(values) => {
                for id in &**values {
                    validate_index(id.to_index(), self.constant_values.len())?;
                }
            }
            InterfaceConstantValueKind::Product(fields) => {
                for field in fields.iter() {
                    validate_symbol(field.field(), symbol_count, dependency_count)?;
                    validate_index(field.value().to_index(), self.constant_values.len())?;
                }
            }
            InterfaceConstantValueKind::Union { variant, fields } => {
                validate_symbol(variant, symbol_count, dependency_count)?;

                for field in fields.iter() {
                    validate_symbol(field.field(), symbol_count, dependency_count)?;
                    validate_index(field.value().to_index(), self.constant_values.len())?;
                }
            }
            InterfaceConstantValueKind::String(value) => {
                limits.check(InterfaceLimit::StringLength, saturating_u64(value.len()))?;
            }
            InterfaceConstantValueKind::Boolean(_)
            | InterfaceConstantValueKind::Character(_)
            | InterfaceConstantValueKind::Integer(_)
            | InterfaceConstantValueKind::Real(_)
            | InterfaceConstantValueKind::Complex { .. }
            | InterfaceConstantValueKind::Unit
            | InterfaceConstantValueKind::NullableAbsent => {}
        }

        Ok(())
    }

    fn validate_constant_term(
        &self,
        term: &InterfaceConstantTerm,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        match term {
            InterfaceConstantTerm::Value(id) => {
                validate_index(id.to_index(), self.constant_values.len())?;
            }
            InterfaceConstantTerm::IntegerLiteral { value, .. } => {
                limits.check(
                    InterfaceLimit::BlobLength,
                    saturating_u64(value.magnitude().len()),
                )?;
            }
            InterfaceConstantTerm::Parameter(symbol)
            | InterfaceConstantTerm::TargetProperty(symbol) => {
                validate_symbol(symbol, symbol_count, dependency_count)?;
            }
            InterfaceConstantTerm::Unary { operand, .. } => {
                validate_index(operand.to_index(), self.constant_terms.len())?;
            }
            InterfaceConstantTerm::Binary { left, right, .. } => {
                validate_index(left.to_index(), self.constant_terms.len())?;
                validate_index(right.to_index(), self.constant_terms.len())?;
            }
            InterfaceConstantTerm::Conversion { operand, target } => {
                validate_index(operand.to_index(), self.constant_terms.len())?;
                validate_index(target.to_index(), self.types.len())?;
            }
            InterfaceConstantTerm::NullablePresent(value) => {
                validate_index(value.to_index(), self.constant_terms.len())?;
            }
            InterfaceConstantTerm::Tuple(values) | InterfaceConstantTerm::Array(values) => {
                for value in values.iter() {
                    validate_index(value.to_index(), self.constant_terms.len())?;
                }
            }
            InterfaceConstantTerm::Product(fields) => {
                for field in fields.iter() {
                    validate_symbol(field.field(), symbol_count, dependency_count)?;
                    validate_index(field.value().to_index(), self.constant_terms.len())?;
                }
            }
            InterfaceConstantTerm::Union { variant, fields } => {
                validate_symbol(variant, symbol_count, dependency_count)?;

                for field in fields.iter() {
                    validate_symbol(field.field(), symbol_count, dependency_count)?;
                    validate_index(field.value().to_index(), self.constant_terms.len())?;
                }
            }
            InterfaceConstantTerm::DefinitionApplication {
                definition,
                substitution,
                selected_implementation,
            } => {
                validate_symbol(definition, symbol_count, dependency_count)?;
                validate_index(substitution.to_index(), self.substitutions.len())?;

                if let Some(id) = selected_implementation {
                    validate_index(id.to_index(), self.implementation_instances.len())?;
                }
            }
            InterfaceConstantTerm::Call {
                callable,
                selected_implementation,
                arguments,
            } => {
                validate_index(callable.to_index(), self.callable_instances.len())?;

                if let Some(id) = selected_implementation {
                    validate_index(id.to_index(), self.implementation_instances.len())?;
                }

                for id in &**arguments {
                    validate_index(id.to_index(), self.constant_terms.len())?;
                }
            }
            InterfaceConstantTerm::PredicateCall {
                predicate,
                substitution,
                arguments,
            } => {
                validate_symbol(predicate, symbol_count, dependency_count)?;
                validate_index(substitution.to_index(), self.substitutions.len())?;

                for id in &**arguments {
                    validate_index(id.to_index(), self.constant_terms.len())?;
                }
            }
            InterfaceConstantTerm::Projection { subject, kind } => {
                validate_index(subject.to_index(), self.constant_terms.len())?;
                self.validate_constant_projection(kind, symbol_count, dependency_count)?;
            }
        }

        Ok(())
    }

    fn validate_constant_projection(
        &self,
        projection: &InterfaceConstantProjection,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        match projection {
            InterfaceConstantProjection::ArrayElement(id) => {
                validate_index(id.to_index(), self.constant_terms.len())?;
            }
            InterfaceConstantProjection::ProductField(symbol)
            | InterfaceConstantProjection::UnionPayloadField(symbol) => {
                validate_symbol(symbol, symbol_count, dependency_count)?;
            }
            InterfaceConstantProjection::TupleElement(_)
            | InterfaceConstantProjection::NullableValue => {}
        }

        Ok(())
    }

    fn validate_dependency_requirement(
        &self,
        requirement: &InterfaceDependencyRequirement,
        symbol_count: usize,
        dependency_count: usize,
        depth: u64,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        limits.check(InterfaceLimit::SemanticTypeDepth, depth)?;

        match &requirement.value {
            InterfaceDependencyRequirementValue::Direct { subject, .. } => {
                self.validate_dependency_subject(subject, symbol_count, dependency_count)?;
            }
            InterfaceDependencyRequirementValue::Guarded {
                guard,
                requirements,
            } => {
                self.validate_dependency_guard(guard, symbol_count, dependency_count)?;

                for nested in &**requirements {
                    self.validate_dependency_requirement(
                        nested,
                        symbol_count,
                        dependency_count,
                        depth.saturating_add(1),
                        limits,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn validate_dependency_subject(
        &self,
        subject: &InterfaceDependencySubject,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        if let InterfaceDependencySubjectRoot::ImplementationWitness(id) = subject.root {
            validate_index(id.to_index(), self.implementation_instances.len())?;
        }

        for projection in &*subject.projections {
            match projection {
                InterfaceDependencyProjection::ProductField(symbol)
                | InterfaceDependencyProjection::UnionPayloadField(symbol) => {
                    validate_symbol(symbol, symbol_count, dependency_count)?;
                }
                InterfaceDependencyProjection::Element(id) => {
                    validate_index(id.to_index(), self.constant_terms.len())?;
                }
                InterfaceDependencyProjection::TupleElement(_)
                | InterfaceDependencyProjection::NullableValue
                | InterfaceDependencyProjection::OwnedTarget => {}
            }
        }

        Ok(())
    }

    fn validate_dependency_guard(
        &self,
        guard: &InterfaceDependencyGuard,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        match guard {
            InterfaceDependencyGuard::NullablePresent(subject) => {
                self.validate_dependency_subject(subject, symbol_count, dependency_count)
            }
            InterfaceDependencyGuard::ActiveUnionVariant { subject, variant } => {
                self.validate_dependency_subject(subject, symbol_count, dependency_count)?;

                validate_symbol(variant, symbol_count, dependency_count)
            }
        }
    }
}
fn validate_type_depth(
    semantics: &InterfaceSemantics,
    limits: InterfaceValidationLimits,
) -> Result<(), InterfaceValidationError> {
    #[derive(Clone, Copy)]
    enum VisitState {
        Unvisited,
        Visiting,
        Complete(u64),
    }

    let mut states = vec![VisitState::Unvisited; semantics.types.len()];

    for root in 0..semantics.types.len() {
        if matches!(states[root], VisitState::Complete(_)) {
            continue;
        }

        let mut pending = vec![(root, false)];

        while let Some((index, exiting)) = pending.pop() {
            let Some(ty) = semantics.types.get(index) else {
                return Err(InterfaceValidationError::Malformed);
            };

            if exiting {
                let mut depth = 1_u64;

                for child in direct_type_children(ty) {
                    let Some(child) = child.to_index() else {
                        return Err(InterfaceValidationError::Malformed);
                    };

                    let Some(VisitState::Complete(child_depth)) = states.get(child).copied() else {
                        return Err(InterfaceValidationError::Malformed);
                    };

                    depth = depth.max(child_depth.saturating_add(1));
                }

                limits.check(InterfaceLimit::SemanticTypeDepth, depth)?;
                states[index] = VisitState::Complete(depth);

                continue;
            }

            match states[index] {
                VisitState::Complete(_) => continue,
                VisitState::Visiting => return Err(InterfaceValidationError::Malformed),
                VisitState::Unvisited => states[index] = VisitState::Visiting,
            }

            pending.push((index, true));

            for child in direct_type_children(ty).into_iter().rev() {
                let Some(index) = child.to_index() else {
                    return Err(InterfaceValidationError::Malformed);
                };

                match states.get(index).copied() {
                    Some(VisitState::Unvisited) => pending.push((index, false)),
                    Some(VisitState::Visiting) => {
                        return Err(InterfaceValidationError::Malformed);
                    }
                    Some(VisitState::Complete(_)) => {}
                    None => return Err(InterfaceValidationError::Malformed),
                }
            }
        }
    }

    Ok(())
}

fn direct_type_children(ty: &InterfaceType) -> Vec<InterfaceTypeId> {
    match ty {
        InterfaceType::Tuple(elements) => elements.to_vec(),
        InterfaceType::Array { element, .. }
        | InterfaceType::FlexibleArray(element)
        | InterfaceType::Slice(element)
        | InterfaceType::Generator(element)
        | InterfaceType::Nullable(element)
        | InterfaceType::Borrow {
            target: element, ..
        } => vec![*element],
        InterfaceType::OwnedIndirection { storage, target } => vec![*storage, *target],
        InterfaceType::Callable {
            parameters, result, ..
        } => parameters
            .iter()
            .map(|parameter| parameter.ty)
            .chain([*result])
            .collect(),
        InterfaceType::Named { .. }
        | InterfaceType::TypeParameter(_)
        | InterfaceType::ContextualSelf(_)
        | InterfaceType::TraitView(_) => Vec::new(),
        InterfaceType::TypeValuedMemberProjection { subject, .. } => vec![*subject],
    }
}
