use std::sync::Arc;

use bray_symbols::{
    CallableParameterData, CallableParameterName, GenericTypeParameterSymbolId, NamedTypeSymbolId,
    SelfTypeContext, SemanticValueStore, TraitTypeMemberSymbolId, TypeData,
};

use crate::{InterfaceSemanticFacts, InterfaceType};

use super::common::{collect_ids, invalid_symbol, resolve_exact, resolve_family, resolve_symbol};
use super::{InterfaceSemanticInternError, InterfaceSymbolResolver, InternState};

impl InternState {
    pub(super) fn intern_types(
        &mut self,
        facts: &InterfaceSemanticFacts,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in facts.types.iter().enumerate() {
            if self.types[index].is_some() {
                continue;
            }

            let Some(data) = self.convert_type(input, symbols)? else {
                continue;
            };

            self.types[index] = Some(store.intern_type(data)?);
        }

        Ok(())
    }

    pub(super) fn convert_type(
        &self,
        input: &InterfaceType,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<TypeData>, InterfaceSemanticInternError> {
        Ok(match input {
            InterfaceType::Named {
                definition,
                substitution,
            } => {
                let Some(substitution) = self.substitution_id(*substitution) else {
                    return Ok(None);
                };

                Some(TypeData::Named {
                    definition: resolve_family::<NamedTypeSymbolId>(symbols, definition)?,
                    substitution,
                })
            }
            InterfaceType::TypeParameter(parameter) => {
                Some(TypeData::TypeParameter(resolve_exact::<
                    GenericTypeParameterSymbolId,
                >(
                    symbols, parameter
                )?))
            }
            InterfaceType::ContextualSelf(context) => {
                let symbol = resolve_symbol(symbols, context)?;
                let Some(context) = SelfTypeContext::try_new(symbol) else {
                    return Err(invalid_symbol(context));
                };

                Some(TypeData::ContextualSelf(context))
            }
            InterfaceType::AssociatedTypeProjection {
                application,
                member,
            } => {
                let Some(application) = self.trait_application_id(*application) else {
                    return Ok(None);
                };

                Some(TypeData::AssociatedTypeProjection {
                    application,
                    member: resolve_exact::<TraitTypeMemberSymbolId>(symbols, member)?,
                })
            }
            InterfaceType::Tuple(elements) => {
                let Some(elements) = collect_ids(elements, |id| self.type_id(*id)) else {
                    return Ok(None);
                };

                Some(TypeData::tuple(elements))
            }
            InterfaceType::Array { element, length } => {
                let (Some(element), Some(length)) =
                    (self.type_id(*element), self.constant_term_id(*length))
                else {
                    return Ok(None);
                };

                Some(TypeData::Array { element, length })
            }
            InterfaceType::Slice(id) => self.type_id(*id).map(TypeData::Slice),
            InterfaceType::Nullable(id) => self.type_id(*id).map(TypeData::Nullable),
            InterfaceType::Borrow { kind, target } => {
                self.type_id(*target).map(|target| TypeData::Borrow {
                    kind: *kind,
                    target,
                })
            }
            InterfaceType::TraitView(id) => self.trait_application_id(*id).map(TypeData::TraitView),
            InterfaceType::OwnedIndirection { storage, target } => {
                let (Some(storage), Some(target)) = (self.type_id(*storage), self.type_id(*target))
                else {
                    return Ok(None);
                };

                Some(TypeData::OwnedIndirection { storage, target })
            }
            InterfaceType::Callable {
                parameters,
                result,
                constness,
                execution,
                trust,
                abi,
                invocation_dependency_contract,
                deferred_dependency_contract,
            } => {
                let Some(result) = self.type_id(*result) else {
                    return Ok(None);
                };

                let Some(invocation_dependency_contract) =
                    self.dependency_contract_id(*invocation_dependency_contract)
                else {
                    return Ok(None);
                };

                let deferred_dependency_contract = match deferred_dependency_contract {
                    Some(contract) => {
                        let Some(contract) = self.dependency_contract_id(*contract) else {
                            return Ok(None);
                        };

                        contract
                    }
                    None => invocation_dependency_contract,
                };

                let mut converted = Vec::with_capacity(parameters.len());

                for parameter in &**parameters {
                    let Some(ty) = self.type_id(parameter.ty) else {
                        return Ok(None);
                    };

                    let Some(name) = CallableParameterName::try_new(Arc::clone(&parameter.name))
                    else {
                        return Err(InterfaceSemanticInternError::UnresolvedValueGraph);
                    };

                    converted.push(CallableParameterData::new(
                        name,
                        parameter.position,
                        parameter.mode,
                        ty,
                    ));
                }

                Some(TypeData::Callable(bray_symbols::CallableTypeData::new(
                    converted,
                    result,
                    *constness,
                    *trust,
                    *abi,
                    bray_symbols::CallableDependencyContracts::for_execution(
                        *execution,
                        invocation_dependency_contract,
                        deferred_dependency_contract,
                    ),
                )))
            }
        })
    }
}
