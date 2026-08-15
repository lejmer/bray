use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, GenericArgument, GenericOwnerId,
    GenericParameterSymbolId, GenericSubstitutionData, ImplementationInstanceData,
    ImplementationSymbolId, SemanticValueStore, TraitApplicationData, TraitSymbolId,
};

use crate::{InterfaceGenericArgument, InterfaceSemantics};

use super::common::{invalid_symbol, resolve_exact, resolve_symbol};
use super::{InterfaceSemanticInternError, InterfaceSymbolResolver, InternState};

impl InternState {
    pub(super) fn intern_substitutions(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.substitutions.iter().enumerate() {
            if self.substitutions[index].is_some() {
                continue;
            }

            let owner_symbol = resolve_symbol(symbols, &input.owner)?;

            let Some(owner) = GenericOwnerId::try_new(owner_symbol) else {
                return Err(invalid_symbol(&input.owner));
            };

            let mut parameters = Vec::with_capacity(input.bindings.len());
            let mut arguments = Vec::with_capacity(input.bindings.len());
            let mut ready = true;

            for binding in &*input.bindings {
                let symbol = resolve_symbol(symbols, &binding.parameter)?;

                let Some(parameter) = GenericParameterSymbolId::try_from_any(symbol) else {
                    return Err(invalid_symbol(&binding.parameter));
                };

                let argument = match binding.argument {
                    InterfaceGenericArgument::Type(id) => {
                        self.type_id(id).map(GenericArgument::Type)
                    }
                    InterfaceGenericArgument::Constant(id) => {
                        self.constant_term_id(id).map(GenericArgument::Constant)
                    }
                };

                let Some(argument) = argument else {
                    ready = false;
                    break;
                };

                parameters.push(parameter);
                arguments.push(argument);
            }

            if !ready {
                continue;
            }

            let data = GenericSubstitutionData::try_new(owner, parameters, arguments)
                .map_err(|_| InterfaceSemanticInternError::UnresolvedValueGraph)?;

            self.substitutions[index] = Some(store.intern_generic_substitution(data)?);
        }

        Ok(())
    }

    pub(super) fn intern_trait_applications(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.trait_applications.iter().enumerate() {
            if self.trait_applications[index].is_some() {
                continue;
            }

            let Some(substitution) = self.substitution_id(input.substitution) else {
                continue;
            };

            let definition = resolve_exact::<TraitSymbolId>(symbols, &input.definition)?;

            self.trait_applications[index] =
                Some(store.intern_trait_application(TraitApplicationData::new(
                    definition,
                    substitution,
                ))?);
        }

        Ok(())
    }

    pub(super) fn intern_callable_instances(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.callable_instances.iter().enumerate() {
            if self.callable_instances[index].is_some() {
                continue;
            }

            let Some(substitution) = self.substitution_id(input.substitution) else {
                continue;
            };

            let symbol = resolve_symbol(symbols, &input.definition)?;

            let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                return Err(invalid_symbol(&input.definition));
            };

            self.callable_instances[index] =
                Some(store.intern_callable_instance(CallableInstanceData::new(
                    definition,
                    substitution,
                ))?);
        }

        Ok(())
    }

    pub(super) fn intern_implementation_instances(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.implementation_instances.iter().enumerate() {
            if self.implementation_instances[index].is_some() {
                continue;
            }

            let Some(substitution) = self.substitution_id(input.substitution) else {
                continue;
            };

            let symbol = resolve_symbol(symbols, &input.definition)?;

            let Some(definition) = ImplementationSymbolId::try_from_any(symbol) else {
                return Err(invalid_symbol(&input.definition));
            };

            self.implementation_instances[index] = Some(store.intern_implementation_instance(
                ImplementationInstanceData::new(definition, substitution),
            )?);
        }

        Ok(())
    }
}
