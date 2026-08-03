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
            InterfaceType::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                let Some(subject) = self.type_id(*subject) else {
                    return Ok(None);
                };

                let Some(application) = self.trait_application_id(*application) else {
                    return Ok(None);
                };

                Some(TypeData::TypeValuedMemberProjection {
                    subject,
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
            InterfaceType::Generator(id) => self.type_id(*id).map(TypeData::Generator),
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
                trust,
                abi,
                invocation_behavior,
                deferred_execution_behavior,
            } => {
                let Some(result) = self.type_id(*result) else {
                    return Ok(None);
                };

                let invocation_behavior =
                    self.convert_callable_behavior(invocation_behavior, symbols)?;

                let deferred_execution_behavior = deferred_execution_behavior
                    .as_ref()
                    .map(|behavior| self.convert_callable_behavior(behavior, symbols))
                    .transpose()?;

                let phase_behaviors = match deferred_execution_behavior {
                    Some(deferred) => bray_symbols::CallablePhaseBehaviors::asynchronous(
                        invocation_behavior,
                        deferred,
                    ),
                    None => bray_symbols::CallablePhaseBehaviors::synchronous(invocation_behavior),
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

                let dependencies = phase_behaviors.dependency_contracts();

                Some(TypeData::Callable(
                    bray_symbols::CallableTypeData::new(
                        converted,
                        result,
                        *constness,
                        *trust,
                        *abi,
                        dependencies,
                    )
                    .with_phase_behaviors(phase_behaviors),
                ))
            }
        })
    }
}
