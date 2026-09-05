use bray_binder::SymbolQueryProvider;
use bray_ir::{MirCallableReference, MirPlace, MirProjectionKind};
use bray_symbols::{
    CallableExecution, CallableSignatureQuery, NamedTypeSymbolId, SymbolQueryRequest,
    TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::super::super::super::CodegenPreparationError;
use super::super::super::super::super::Compilation;
use super::super::super::super::super::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductValueKind,
};
use super::super::super::support::{
    closed_array_length, projected_lifecycle_place, receiver_codegen_type,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation::product::realization) fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<
        Option<(MirCallableReference, TypeId, TypeId, CallableExecution)>,
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        let surface =
            self.type_associated_surface_result_with_cancellation(*definition, cancellation)?;

        let members = surface
            .value()
            .lifecycle_members()
            .iter()
            .filter(|member| member.slot() == slot)
            .map(|member| member.id())
            .collect::<Vec<_>>();

        let [member] = members.as_slice() else {
            if members.is_empty() {
                return Ok(None);
            }

            return Err(ProductQueryFailure::count_mismatch(
                ProductQueryContext::Type(ty),
                ProductDataKind::LifecycleMember,
                1,
                members.len(),
            )
            .into());
        };

        let member = *member;

        let callable = super::super::super::super::super::implementation::callable_instance(
            values,
            member,
            [*substitution],
        )?;

        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(super::super::super::super::super::binder::binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature.value().callable_type(),
                signature.value().result(),
            ],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            values,
            signature.value(),
            callable.substitution(),
            constants.value(),
        )
        .map_err(FactQueryError::from)?
        .ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::CallableData(callable),
                ProductDataKind::CallableSignature,
            )
        })?;

        let receiver = signature.receiver().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::CallableData(callable),
                ProductDataKind::CallableReceiver,
            )
        })?;

        let receiver_ty =
            self.concrete_codegen_type(receiver.ty(), Some(*substitution), None, cancellation)?;

        let receiver = receiver_codegen_type(values, receiver_ty, receiver.mode())?;

        let result = self.concrete_codegen_type(
            signature.result(),
            Some(*substitution),
            None,
            cancellation,
        )?;

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty: signature.callable_type(),
                expected: ProductValueKind::CallableType,
                actual: callable_type.as_ref().clone(),
            }
            .into());
        };

        Ok(Some((
            MirCallableReference::new(callable, callable_type.abi()),
            receiver,
            result,
            callable_type.execution(),
        )))
    }

    pub(in crate::compilation::product::realization) fn lifecycle_children(
        &self,
        place: MirPlace,
        cancellation: &CancellationToken,
    ) -> Result<Vec<MirPlace>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(place.ty())
            .map_err(FactQueryError::SemanticValueStore)?;

        let children = match data.as_ref() {
            TypeData::Named {
                definition: NamedTypeSymbolId::Struct(structure),
                substitution,
            } => {
                if super::super::super::super::super::foreign::compiler_known_representation(
                    self,
                    NamedTypeSymbolId::Struct(*structure),
                )
                .is_some()
                {
                    return Err(CodegenPreparationError::UnsupportedType(place.ty()));
                }

                let representation = self.declared_type_representation_with_cancellation(
                    NamedTypeSymbolId::Struct(*structure),
                    cancellation,
                )?;

                let bray_symbols::DeclaredStorageShape::Structure(members) =
                    representation.value().storage()
                else {
                    return Err(CodegenPreparationError::UnresolvedType(place.ty()));
                };

                members
                    .iter()
                    .enumerate()
                    .map(|(index, member)| {
                        let ty =
                            self.resolve_codegen_type(member.ty(), *substitution, cancellation)?;

                        let projection =
                            MirProjectionKind::TupleField(u32::try_from(index).map_err(|_| {
                                CodegenPreparationError::LayoutOverflow(place.ty())
                            })?);

                        Ok((projection, ty))
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?
            }
            TypeData::Tuple(elements) => elements
                .iter()
                .copied()
                .enumerate()
                .map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| CodegenPreparationError::LayoutOverflow(place.ty()))?;

                    Ok((MirProjectionKind::TupleField(index), ty))
                })
                .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
            TypeData::Array { element, length } => {
                let length = closed_array_length(values, *length)?;

                let length = u32::try_from(length)
                    .map_err(|_| CodegenPreparationError::LayoutOverflow(place.ty()))?;

                (0..length)
                    .map(|index| (MirProjectionKind::ElementFromStart(index), *element))
                    .collect()
            }
            TypeData::Error
            | TypeData::Named {
                definition: NamedTypeSymbolId::Union(_),
                ..
            }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::Borrow { .. }
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => {
                return Err(CodegenPreparationError::UnsupportedType(place.ty()));
            }
        };

        Ok(children
            .into_iter()
            .map(|(kind, ty)| projected_lifecycle_place(&place, kind, ty))
            .collect())
    }
}
