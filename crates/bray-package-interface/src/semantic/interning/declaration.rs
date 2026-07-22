use bray_symbols::{
    CallableParameterSymbolId, CallableSignatureTemplate, CallableSymbolId,
    GenericConstraintTemplate, GenericDeclarationTemplate, GenericOwnerId,
    GenericParameterSymbolId, PredicateDefinitionSymbolId, ReceiverParameterSignature,
    ReceiverParameterSymbolId, TypeExpressionTemplate, UnevaluatedDefaultTemplate,
};

use super::common::{invalid_symbol, resolve_exact, resolve_family, resolve_symbol};
use super::{
    ImportedCallableParameterDefaultFact, ImportedCallableSignatureFact, ImportedConstraintFact,
    ImportedGenericDeclarationFact, ImportedPredicateDefinitionFact, InterfaceSemanticInternError,
    InterfaceSymbolResolver, InternState,
};
use crate::InterfaceSemanticFacts;

impl InternState {
    pub(super) fn convert_callable_signatures(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedCallableSignatureFact>, InterfaceSemanticInternError> {
        facts
            .callable_signatures
            .iter()
            .map(|input| {
                let callable_type = self
                    .type_id(input.callable_type())
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                let result = self
                    .type_id(input.result())
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                let receiver = input
                    .receiver()
                    .map(|receiver| {
                        let parameter = resolve_exact::<ReceiverParameterSymbolId>(
                            symbols,
                            receiver.parameter(),
                        )?;

                        let ty = self
                            .type_id(receiver.ty())
                            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                        Ok::<_, InterfaceSemanticInternError>(ReceiverParameterSignature::new(
                            parameter,
                            ty,
                            receiver.mode(),
                        ))
                    })
                    .transpose()?;

                let parameters = input
                    .parameters()
                    .iter()
                    .map(|parameter| resolve_exact::<CallableParameterSymbolId>(symbols, parameter))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(ImportedCallableSignatureFact {
                    owner: resolve_family::<CallableSymbolId>(symbols, input.owner())?,
                    signature: CallableSignatureTemplate::new(
                        TypeExpressionTemplate::Resolved(callable_type),
                        receiver,
                        parameters,
                        TypeExpressionTemplate::Resolved(result),
                    ),
                })
            })
            .collect()
    }

    pub(super) fn convert_generic_declarations(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
        constraints: &[ImportedConstraintFact],
    ) -> Result<Vec<ImportedGenericDeclarationFact>, InterfaceSemanticInternError> {
        facts
            .generic_declarations
            .iter()
            .map(|input| {
                let owner = resolve_symbol(symbols, input.owner())?;

                let Some(owner) = GenericOwnerId::try_new(owner) else {
                    return Err(invalid_symbol(input.owner()));
                };

                let parameters = input
                    .parameters()
                    .iter()
                    .map(|parameter| resolve_family::<GenericParameterSymbolId>(symbols, parameter))
                    .collect::<Result<Vec<_>, _>>()?;

                let constraints = constraints
                    .iter()
                    .filter(|constraint| constraint.owner() == owner)
                    .map(|constraint| GenericConstraintTemplate::Resolved(constraint.constraint()));

                Ok(ImportedGenericDeclarationFact {
                    owner,
                    declaration: GenericDeclarationTemplate::new(owner, parameters, constraints),
                })
            })
            .collect()
    }

    pub(super) fn convert_callable_parameter_defaults(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedCallableParameterDefaultFact>, InterfaceSemanticInternError> {
        facts
            .callable_parameter_defaults
            .iter()
            .map(|input| {
                let default = if input.is_present() {
                    UnevaluatedDefaultTemplate::Resolved
                } else {
                    UnevaluatedDefaultTemplate::Absent
                };

                Ok(ImportedCallableParameterDefaultFact {
                    parameter: resolve_exact::<CallableParameterSymbolId>(
                        symbols,
                        input.parameter(),
                    )?,
                    default,
                })
            })
            .collect()
    }

    pub(super) fn convert_predicate_definitions(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedPredicateDefinitionFact>, InterfaceSemanticInternError> {
        facts
            .predicate_definitions
            .iter()
            .map(|input| {
                let owner = resolve_symbol(symbols, input.owner())?;

                let Some(owner) = PredicateDefinitionSymbolId::try_from_any(owner) else {
                    return Err(invalid_symbol(input.owner()));
                };

                Ok(ImportedPredicateDefinitionFact {
                    owner,
                    state: input.state(),
                })
            })
            .collect()
    }
}
