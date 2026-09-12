use bray_symbols::{
    CallableParameterSymbolId, CallableSignatureTemplate, CallableSymbolId, DeclaredStorageShape,
    DeclaredStructStorageMember, DeclaredUnionStorageMember, DeclaredUnionStorageVariant,
    GenericConstraintTemplate, GenericDeclarationTemplate, GenericOwnerId,
    GenericParameterSymbolId, GenericTypeParameterSymbolId, NamedTypeSymbolId,
    PredicateDefinitionSymbolId, ReceiverParameterSignature, ReceiverParameterSymbolId,
    StructFieldSymbolId, TypeExpressionTemplate, UnevaluatedDefaultTemplate,
    UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use super::common::{invalid_symbol, resolve_exact, resolve_family, resolve_symbol};
use super::{
    ImportedCallableParameterDefault, ImportedCallableSignature, ImportedConstraint,
    ImportedDeclaredType, ImportedGenericDeclaration, ImportedPredicateDefinition,
    InterfaceSemanticInternError, InterfaceSymbolResolver, InternState,
};
use crate::InterfaceSemantics;

impl InternState {
    pub(super) fn convert_declared_types(
        &self,
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedDeclaredType>, InterfaceSemanticInternError> {
        semantics
            .declared_types
            .iter()
            .map(|input| {
                let ty = self
                    .type_id(input.ty())
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                Ok(ImportedDeclaredType {
                    owner: resolve_symbol(symbols, input.owner())?,
                    ty,
                })
            })
            .collect()
    }

    pub(super) fn convert_callable_signatures(
        &self,
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedCallableSignature>, InterfaceSemanticInternError> {
        semantics
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

                Ok(ImportedCallableSignature {
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
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
        constraints: &[ImportedConstraint],
    ) -> Result<Vec<ImportedGenericDeclaration>, InterfaceSemanticInternError> {
        semantics
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

                Ok(ImportedGenericDeclaration {
                    owner,
                    declaration: GenericDeclarationTemplate::new(owner, parameters, constraints),
                })
            })
            .collect()
    }

    pub(super) fn convert_callable_parameter_defaults(
        &self,
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedCallableParameterDefault>, InterfaceSemanticInternError> {
        semantics
            .callable_parameter_defaults
            .iter()
            .map(|input| {
                let default = if input.is_present() {
                    UnevaluatedDefaultTemplate::Resolved
                } else {
                    UnevaluatedDefaultTemplate::Absent
                };

                Ok(ImportedCallableParameterDefault {
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
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedPredicateDefinition>, InterfaceSemanticInternError> {
        semantics
            .predicate_definitions
            .iter()
            .map(|input| {
                let owner = resolve_symbol(symbols, input.owner())?;

                let Some(owner) = PredicateDefinitionSymbolId::try_from_any(owner) else {
                    return Err(invalid_symbol(input.owner()));
                };

                Ok(ImportedPredicateDefinition {
                    owner,
                    state: input.state(),
                    signature: bray_symbols::PredicateSignatureTemplate::new(
                        owner,
                        input
                            .parameters()
                            .iter()
                            .map(|(parameter, name, ty)| {
                                Ok(bray_symbols::PredicateParameterTemplate::new(
                                    resolve_exact::<bray_symbols::PredicateParameterSymbolId>(
                                        symbols, parameter,
                                    )?,
                                    name.clone(),
                                    TypeExpressionTemplate::Resolved(self.type_id(*ty).ok_or(
                                        InterfaceSemanticInternError::UnresolvedValueGraph,
                                    )?),
                                ))
                            })
                            .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?,
                        input.state() == crate::InterfacePredicateDefinitionState::OpaqueTrusted,
                    ),
                })
            })
            .collect()
    }

    pub(super) fn convert_type_representations(
        &self,
        semantics: &InterfaceSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<bray_symbols::DeclaredTypeRepresentation>, InterfaceSemanticInternError> {
        semantics
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

                let storage = match input.storage() {
                    crate::InterfaceStorageShape::Structure(members) => {
                        DeclaredStorageShape::Structure(
                            members
                                .iter()
                                .map(|member| {
                                    let field = member
                                        .field()
                                        .map(|field| {
                                            resolve_exact::<StructFieldSymbolId>(symbols, field)
                                        })
                                        .transpose()?;

                                    let ty = self.type_id(member.ty()).ok_or(
                                        InterfaceSemanticInternError::UnresolvedValueGraph,
                                    )?;

                                    Ok(DeclaredStructStorageMember::new(
                                        field,
                                        TypeExpressionTemplate::Resolved(ty),
                                    ))
                                })
                                .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?
                                .into(),
                        )
                    }
                    crate::InterfaceStorageShape::Union(variants) => DeclaredStorageShape::Union(
                        variants
                            .iter()
                            .map(|variant| {
                                let variant_id = resolve_exact::<UnionVariantSymbolId>(
                                    symbols,
                                    variant.variant(),
                                )?;

                                let members = variant
                                    .members()
                                    .iter()
                                    .map(|member| {
                                        let field = member
                                            .field()
                                            .map(|field| {
                                                resolve_exact::<UnionPayloadFieldSymbolId>(
                                                    symbols, field,
                                                )
                                            })
                                            .transpose()?;

                                        let ty = self.type_id(member.ty()).ok_or(
                                            InterfaceSemanticInternError::UnresolvedValueGraph,
                                        )?;

                                        Ok(DeclaredUnionStorageMember::new(
                                            field,
                                            TypeExpressionTemplate::Resolved(ty),
                                        ))
                                    })
                                    .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?;

                                Ok(DeclaredUnionStorageVariant::new(variant_id, members))
                            })
                            .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?
                            .into(),
                    ),
                };

                Ok(bray_symbols::DeclaredTypeRepresentation::new(subject)
                    .with_layout(
                        input.layout(),
                        input.alignment(),
                        input.packing(),
                        union_tag_type,
                    )
                    .with_union_tags(union_tags)
                    .with_opaque_size(input.opaque_size())
                    .with_incomplete(input.is_incomplete())
                    .with_tagless_union(input.is_tagless_union())
                    .with_storage(storage)
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
