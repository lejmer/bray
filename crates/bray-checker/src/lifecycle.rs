use std::sync::Arc;

use bray_bound_tree::{LifecycleAction, LifecycleCallable, LifecyclePhase};
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_symbols::{
    DeclaredStorageShape, DeclaredTypeRepresentation, GenericSubstitutionId, NamedTypeSymbolId,
    SemanticValueStore, TypeAssociatedLifecycleSlot, TypeData, TypeExpressionTemplate, TypeId,
};

/// Semantic queries required to select a local lifecycle action.
pub trait LifecycleSelectionContext {
    /// Retains query failures and invalid semantic inputs.
    type Error: From<LifecycleSelectionError>;

    /// Returns interned semantic values.
    fn semantic_values(&self) -> &SemanticValueStore;
    /// Returns the selected whole-value implementation, if present.
    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
    ) -> Result<Option<LifecycleCallable>, Self::Error>;
    /// Returns a selected unary storage-policy operation and its substituted signature.
    fn storage_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
    ) -> Result<LifecycleCallable, Self::Error>;
    /// Returns the checked stored representation of a declaration.
    fn declared_representation(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Result<DeclaredTypeRepresentation, Self::Error>;
    /// Substitutes a represented member type.
    fn resolve_type(
        &self,
        template: &TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, Self::Error>;
    /// Recognizes an imported buffer under the standard-library authority contract.
    fn imported_raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<Option<TypeId>, Self::Error>;
    /// Returns a declaration's compiler-known representation role.
    fn representation_role(&self, definition: NamedTypeSymbolId) -> Option<RepresentationRole>;
}

/// A semantic input that cannot select a lifecycle action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleSelectionError {
    /// An internal storage protocol identity is malformed.
    InvalidStorageMemberKey(String),
    /// The type has no supported represented cleanup.
    UnsupportedType(TypeId),
    /// The declared representation remains unresolved.
    UnresolvedType(TypeId),
    /// A borrowed or callable value incorrectly requires destruction.
    UnexpectedType {
        /// Requested type.
        ty: TypeId,
        /// Exact semantic input.
        actual: TypeData,
    },
}

/// Selects only this owner's action, leaving child owners demand-driven.
/// Repeated child types remain separate entries in declaration order.
pub fn select_lifecycle_action<C: LifecycleSelectionContext + ?Sized>(
    context: &C,
    ty: TypeId,
    phase: LifecyclePhase,
) -> Result<LifecycleAction, C::Error> {
    let data = context.semantic_values().type_data(ty);

    if let TypeData::Named {
        definition,
        substitution,
    } = data.as_ref()
    {
        if phase == LifecyclePhase::Destroy
            && let Some(element) =
                context.imported_raw_buffer_element(*definition, *substitution)?
        {
            return Ok(LifecycleAction::RawBuffer(element));
        }

        let release = matches!(phase, LifecyclePhase::Destroy | LifecyclePhase::Resolve);

        match context.representation_role(*definition) {
            Some(RepresentationRole::String) => {
                return Ok(if release {
                    LifecycleAction::ReleaseString
                } else {
                    LifecycleAction::None
                });
            }
            Some(RepresentationRole::PanicReport) => {
                return Ok(if release {
                    LifecycleAction::DestroyReport
                } else {
                    LifecycleAction::None
                });
            }
            Some(RepresentationRole::Task) => return Ok(LifecycleAction::Task),
            _ => {}
        }
    }

    let slot = match phase {
        LifecyclePhase::Finalize => Some(TypeAssociatedLifecycleSlot::Finalizer),
        LifecyclePhase::Destroy => Some(TypeAssociatedLifecycleSlot::Destructor),
        LifecyclePhase::Cancel | LifecyclePhase::Resolve => None,
    };

    if let Some(slot) = slot
        && let Some(callable) = context.lifecycle_callable(ty, slot)?
    {
        return Ok(LifecycleAction::Call(callable));
    }

    match phase {
        LifecyclePhase::Finalize => Ok(LifecycleAction::None),
        LifecyclePhase::Resolve => Ok(LifecycleAction::Resolve),
        LifecyclePhase::Destroy | LifecyclePhase::Cancel => {
            select_represented_action(context, ty, phase, &data)
        }
    }
}

fn select_represented_action<C: LifecycleSelectionContext + ?Sized>(
    context: &C,
    ty: TypeId,
    phase: LifecyclePhase,
    data: &TypeData,
) -> Result<LifecycleAction, C::Error> {
    match data {
        TypeData::Tuple(elements) => Ok(LifecycleAction::Members(Arc::clone(elements))),
        TypeData::Array { element, length } => Ok(LifecycleAction::Array {
            element: *element,
            length: *length,
        }),
        TypeData::Nullable(target) => Ok(LifecycleAction::Nullable(*target)),
        TypeData::OwnedIndirection { storage, target } => {
            let select = |name: &str| {
                let member = CompilerKnownDeclarationKey::try_new(name).ok_or_else(|| {
                    LifecycleSelectionError::InvalidStorageMemberKey(name.to_owned())
                })?;

                context.storage_callable(*storage, *target, &member)
            };

            let borrow = select("StorageBorrowMut")?;

            let teardown = if phase == LifecyclePhase::Destroy {
                Some([select("StorageDestroy")?, select("StorageRelease")?])
            } else {
                None
            };

            Ok(LifecycleAction::Owned {
                storage: *storage,
                target: *target,
                borrow,
                teardown,
            })
        }
        TypeData::Generator(element) => Ok(LifecycleAction::Generator(*element)),
        TypeData::Named {
            definition,
            substitution,
        } => {
            if matches!(definition, NamedTypeSymbolId::Struct(_))
                && context.representation_role(*definition).is_some()
            {
                return Err(LifecycleSelectionError::UnsupportedType(ty).into());
            }

            let representation = context.declared_representation(*definition)?;

            match (definition, representation.storage()) {
                (NamedTypeSymbolId::Struct(_), DeclaredStorageShape::Structure(members)) => {
                    Ok(LifecycleAction::Members(member_types(
                        context,
                        members.iter().map(|member| member.ty()),
                        *substitution,
                    )?))
                }
                (NamedTypeSymbolId::Union(_), DeclaredStorageShape::Union(variants)) => {
                    let alternatives = variants
                        .iter()
                        .map(|variant| {
                            Ok((
                                variant.variant(),
                                member_types(
                                    context,
                                    variant.members().iter().map(|member| member.ty()),
                                    *substitution,
                                )?,
                            ))
                        })
                        .collect::<Result<Arc<[_]>, C::Error>>()?;

                    Ok(LifecycleAction::Alternatives(alternatives))
                }
                _ => Err(LifecycleSelectionError::UnresolvedType(ty).into()),
            }
        }
        TypeData::Borrow { .. } | TypeData::Callable(_) => {
            Err(LifecycleSelectionError::UnexpectedType {
                ty,
                // The failure owns the exact semantic input beyond this query borrow.
                actual: data.clone(),
            }
            .into())
        }
        _ => Err(LifecycleSelectionError::UnsupportedType(ty).into()),
    }
}

fn member_types<'a, C: LifecycleSelectionContext + ?Sized>(
    context: &C,
    members: impl Iterator<Item = &'a TypeExpressionTemplate>,
    substitution: GenericSubstitutionId,
) -> Result<Arc<[TypeId]>, C::Error> {
    members
        .map(|member| context.resolve_type(member, substitution))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{LifecycleSelectionContext, LifecycleSelectionError, select_lifecycle_action};
    use bray_bound_tree::{LifecycleAction, LifecycleCallable, LifecyclePhase};
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{
        ConstantTermData, DeclaredTypeRepresentation, GenericSubstitutionId, IntegerConstant,
        NamedTypeSymbolId, SemanticValueStore, TargetSizedIntegerType, TypeAssociatedLifecycleSlot,
        TypeData, TypeExpressionTemplate, TypeId,
    };

    struct Context(SemanticValueStore);

    impl LifecycleSelectionContext for Context {
        type Error = LifecycleSelectionError;
        fn semantic_values(&self) -> &SemanticValueStore {
            &self.0
        }
        fn lifecycle_callable(
            &self,
            _: TypeId,
            _: TypeAssociatedLifecycleSlot,
        ) -> Result<Option<LifecycleCallable>, Self::Error> {
            Ok(None)
        }
        fn storage_callable(
            &self,
            _: TypeId,
            _: TypeId,
            _: &bray_compiler_known::CompilerKnownDeclarationKey,
        ) -> Result<LifecycleCallable, Self::Error> {
            panic!("no storage policy in this fixture")
        }
        fn declared_representation(
            &self,
            _: NamedTypeSymbolId,
        ) -> Result<DeclaredTypeRepresentation, Self::Error> {
            panic!("no declared type in this fixture")
        }
        fn resolve_type(
            &self,
            _: &TypeExpressionTemplate,
            _: GenericSubstitutionId,
        ) -> Result<TypeId, Self::Error> {
            panic!("no member substitution in this fixture")
        }
        fn imported_raw_buffer_element(
            &self,
            _: NamedTypeSymbolId,
            _: GenericSubstitutionId,
        ) -> Result<Option<TypeId>, Self::Error> {
            panic!("no imported type in this fixture")
        }
        fn representation_role(&self, _: NamedTypeSymbolId) -> Option<RepresentationRole> {
            panic!("no named type in this fixture")
        }
    }

    #[test]
    fn repeated_members_preserve_distinct_ownership_positions() {
        let context = Context(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();

        let ty = context
            .0
            .intern_type(TypeData::tuple([leaf, leaf, leaf]))
            .unwrap();

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Destroy).unwrap(),
            LifecycleAction::Members([leaf, leaf, leaf].into())
        );

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Cancel).unwrap(),
            LifecycleAction::Members([leaf, leaf, leaf].into())
        );
    }

    #[test]
    fn array_selection_retains_symbolic_extent_without_visiting_elements() {
        let context = Context(SemanticValueStore::try_new().unwrap());
        let element = context.0.intern_type(TypeData::tuple([])).unwrap();

        for extent in [0, 1, u64::MAX] {
            let length = context
                .0
                .intern_constant_term(ConstantTermData::IntegerLiteral {
                    ty: TargetSizedIntegerType::Usize,
                    value: IntegerConstant::from_u64(extent),
                })
                .unwrap();

            let ty = context
                .0
                .intern_type(TypeData::Array { element, length })
                .unwrap();

            assert_eq!(
                select_lifecycle_action(&context, ty, LifecyclePhase::Destroy).unwrap(),
                LifecycleAction::Array { element, length }
            );
        }
    }

    #[test]
    fn resolution_keeps_finalization_and_destruction_as_separate_obligations() {
        let context = Context(SemanticValueStore::try_new().unwrap());
        let leaf = context.0.intern_type(TypeData::tuple([])).unwrap();
        let ty = context.0.intern_type(TypeData::Nullable(leaf)).unwrap();

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Finalize).unwrap(),
            LifecycleAction::None
        );

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Resolve).unwrap(),
            LifecycleAction::Resolve
        );

        assert_eq!(
            select_lifecycle_action(&context, ty, LifecyclePhase::Destroy).unwrap(),
            LifecycleAction::Nullable(leaf)
        );
    }
}
