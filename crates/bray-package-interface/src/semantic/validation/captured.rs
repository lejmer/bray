use bray_symbols::{CapturedValueOperation as Op, CapturedValueTemplate as Value};

use crate::{InterfaceDependencyContract, InterfaceDependencyProjection, InterfaceLimit, InterfaceSemantics,
    InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits, PackageInterfaceSurface};

use super::surface::validate_index;

fn invalid() -> InterfaceValidationError {
    crate::semantic::codec::invalid_value(InterfaceValidationField::Dependency)
}

impl InterfaceSemantics {
    pub(super) fn validate_captured_value_graph(&self, contract: &InterfaceDependencyContract,
        symbol_count: usize, dependency_count: usize, limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        if !contract.normal_completion && !contract.captured_values.is_empty() { return Err(invalid()); }

        let mut count = 0u64;

        for outcome in &*contract.captured_values {
            if let Some(guard) = outcome.guard { validate_index(guard.to_index(), self.constant_terms.len())?; }

            if !crate::validation::is_strictly_sorted(&outcome.carriers)
                || !outcome.bindings.windows(2).all(|pair| pair[0].0 < pair[1].0)
            {
                return Err(invalid());
            }

            let mut pending = Vec::new();

            for (destination, value) in outcome.bindings.iter() {
                self.validate_dependency_subject(destination, symbol_count, dependency_count)?;
                pending.push((value, 1u64));
            }

            while let Some((value, depth)) = pending.pop() {
                count = count.saturating_add(1);
                limits.check(InterfaceLimit::RecordCount, count)?;
                limits.check(InterfaceLimit::SemanticTypeDepth, depth)?;

                match value {
                    Value::Input(input) | Value::Data(input) => validate_index(input.to_index(), self.constant_terms.len())?,
                    Value::Static(root) => self.validate_static_reference(root, symbol_count, dependency_count)?,
                    Value::Deferred { alternatives, unknown } => {
                        if alternatives.is_empty() && !unknown { return Err(invalid()); }

                        for alternative in alternatives.iter() {
                            if !alternative.owned_inputs.windows(2).all(|pair| pair[0] < pair[1])
                                || alternative.owned_inputs.iter().any(|ordinal| ordinal.to_index().is_none_or(|index| index >= alternative.captures.len()))
                            { return Err(invalid()); }

                            if let Some(guard) = alternative.guard { validate_index(guard.to_index(), self.constant_terms.len())?; }

                            if let Some(call) = &alternative.call {
                                validate_index(call.instance.to_index(), self.callable_instances.len())?;

                                let Some(crate::InterfaceType::Callable { parameters, variadic, .. }) = call.signature.to_index().and_then(|index| self.types.get(index)) else { return Err(invalid()); };

                                let required = parameters.len() + usize::from(call.has_receiver);

                                if alternative.captures.len() != required && (!variadic || alternative.captures.len() < required) { return Err(invalid()); }

                                if let Some((ty, app)) = call.requirement {
                                    validate_index(ty.to_index(), self.types.len())?;
                                    validate_index(app.to_index(), self.trait_applications.len())?;
                                }

                                for w in call.dispatch.iter().chain(&*call.witnesses) { validate_index(w.to_index(), self.implementation_instances.len())?; }
                            }

                            pending.extend(alternative.captures.iter().map(|value| (value, depth + 1)));
                        }
                    }
                    Value::Unknown => {}
                    Value::Tagged(variant, payload) => {
                        super::surface::validate_symbol(variant, symbol_count, dependency_count)?;
                        pending.push((payload, depth + 1));
                    }
                    Value::Operation(operation, values) => {
                        if operation.operand_count() != values.len() { return Err(invalid()); }

                        if let Op::Select(projection) | Op::OwningStorage(projection) = operation {
                            self.validate_captured_projection(projection, symbol_count, dependency_count)?;
                        }

                        pending.extend(values.iter().map(|value| (value, depth + 1)));
                    }
                    Value::Aggregate { base, fields } => {
                        let mut selected = std::collections::BTreeSet::new();

                        for (field, value) in fields.iter() {
                            if !selected.insert(field) { return Err(invalid()); }

                            self.validate_captured_projection(field, symbol_count, dependency_count)?;
                            pending.push((value, depth + 1));
                        }

                        pending.push((base, depth + 1));
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_captured_projection(&self, projection: &InterfaceDependencyProjection,
        symbol_count: usize, dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        if matches!(projection, InterfaceDependencyProjection::AnyElement | InterfaceDependencyProjection::AddressTarget) {
            return Err(invalid());
        }

        self.validate_dependency_projections(std::slice::from_ref(projection), symbol_count, dependency_count)
    }

    pub(in crate::semantic::validation) fn validate_captured_execution(&self, contract: &InterfaceDependencyContract,
        inputs: usize, surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        for outcome in &*contract.captured_values {
            if let Some(guard) = outcome.guard { self.validate_execution_term(guard, inputs, true, surface)?; }

            for (_, value) in outcome.bindings.iter() {
            for input in value.inputs() {
                self.validate_returned_input(*input, inputs, surface)?;
            }

            for node in value.nodes() {
                match node {
                    Value::Data(term) => self.validate_execution_term(*term, inputs, false, surface)?,
                    Value::Deferred { alternatives, .. } => {
                        for alternative in alternatives.iter() {
                            if let Some(guard) = alternative.guard { self.validate_execution_term(guard, inputs, false, surface)?; }

                            let Some(call) = &alternative.call else { continue; };

                            let instance = call.instance.to_index().and_then(|index| self.callable_instances.get(index)).ok_or_else(invalid)?;
                            let substitution = instance.substitution.to_index().and_then(|index| self.substitutions.get(index)).ok_or_else(invalid)?;

                            if !super::surface::validate_symbol_kind(&instance.definition, surface)?.is_callable() || substitution.owner != instance.definition {
                                return Err(invalid());
                            }
                        }
                    }
                    Value::Tagged(variant, _) if super::surface::validate_symbol_kind(variant, surface)? != bray_symbols::SymbolKind::UnionVariant => return Err(invalid()),
                    Value::Static(root) if super::surface::validate_symbol_kind(&root.declaration, surface)? != bray_symbols::SymbolKind::Static => return Err(invalid()),
                    _ => {}
                }
            }
        }

        }

        Ok(())
    }
}
