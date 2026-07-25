use bray_symbols::{
    CallableParameterSymbolId, CallableSignatureTemplate, CallableSymbolId,
    GenericConstraintTemplate, GenericDeclarationTemplate, GenericOwnerId,
    GenericParameterSymbolId, GenericTypeParameterSymbolId, NamedTypeSymbolId,
    PredicateDefinitionSymbolId, ReceiverParameterSignature, ReceiverParameterSymbolId,
    TypeExpressionTemplate, UnevaluatedDefaultTemplate, UnionVariantSymbolId,
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
                    )
                    .with_body(input.has_body()),
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

    pub(super) fn convert_type_representations(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<bray_symbols::DeclaredTypeRepresentation>, InterfaceSemanticInternError> {
        facts
            .type_representations
            .iter()
            .map(|input| {
                let subject = resolve_family::<NamedTypeSymbolId>(symbols, input.owner())?;

                let union_tag_type = input
                    .union_tag_type()
                    .map(|tag_type| {
                        self.type_id(tag_type)
                            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)
                    })
                    .transpose()?;

                let union_tags = input
                    .union_tags()
                    .iter()
                    .map(|tag| {
                        Ok(bray_symbols::DeclaredUnionTag::new(
                            resolve_exact::<UnionVariantSymbolId>(symbols, tag.variant())?,
                            tag.value().clone(),
                        ))
                    })
                    .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?;

                let copy_dependencies = input
                    .copy_dependencies()
                    .iter()
                    .map(|dependency| {
                        resolve_exact::<GenericTypeParameterSymbolId>(symbols, dependency)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(bray_symbols::DeclaredTypeRepresentation::new(subject)
                    .with_layout(
                        input.layout(),
                        input.alignment(),
                        input.packing(),
                        union_tag_type,
                    )
                    .with_union_tags(union_tags)
                    .with_properties(
                        input.copy_contract(),
                        input.is_plain_storage(),
                        input.has_finite_size(),
                        false,
                    )
                    .with_copy_dependencies(copy_dependencies))
            })
            .collect()
    }
}
