use bray_bound_tree::{
    AsyncStorageCleanupRequirement, BoundSourceAnchor, StorageCleanupPart,
    StorageCleanupProjection, StorageCleanupProjectionKind, StorageProjection, StorageProtocolCall,
};
use bray_symbols::{DeclaredStorageShape, SymbolOrdinal, TypeData, TypeId};

use super::cleanup::{CleanupShapeResolver, cleanup_requirement};
use crate::{CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum CleanupExpansion {
    MovedPaths,
    DestructorReceiver,
    Completion,
}

impl<C: CheckerRequestContext + ?Sized> CleanupShapeResolver<'_, C> {
    pub(super) fn represented_parts(
        &mut self,
        ty: TypeId,
        moved: &[&[StorageProjection]],
        expansion: CleanupExpansion,
        source: BoundSourceAnchor,
    ) -> Result<Option<Vec<StorageCleanupPart>>, CheckerQueryError<C::UpstreamError>> {
        let mut parts = Vec::new();

        if self.append_parts(ty, moved, expansion, source, &mut Vec::new(), &mut parts)? {
            return Ok(Some(parts));
        }

        Ok(None)
    }

    fn append_parts(
        &mut self,
        ty: TypeId,
        moved: &[&[StorageProjection]],
        expansion: CleanupExpansion,
        source: BoundSourceAnchor,
        path: &mut Vec<StorageCleanupProjection>,
        parts: &mut Vec<StorageCleanupPart>,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let cleanup = cleanup_requirement(self.resolve(ty)?);

        self.cleanup_types
            .entry(ty)
            .or_insert_with(|| bray_bound_tree::StorageCleanupType::new(ty, cleanup));

        if expansion == CleanupExpansion::Completion {
            if cleanup == AsyncStorageCleanupRequirement::None {
                return Ok(true);
            }

            if path.iter().any(|projection| projection.source_type() == ty) {
                return Ok(append_whole_part(cleanup, path, parts));
            }
        }

        if expansion == CleanupExpansion::MovedPaths && moved.iter().all(|path| path.is_empty()) {
            return Ok(append_whole_part(cleanup, path, parts));
        }

        let data = self
            .context
            .semantic_values()
            .type_data(ty)
            .map_err(CheckerInfrastructureError::SemanticValueStore)
            .map_err(CheckerQueryError::Infrastructure)?;

        let mut release = None;
        let mut requires_whole_value = false;

        let children = match data.as_ref() {
            TypeData::OwnedIndirection { storage, target } => {
                let Some(borrow) = self.storage_call(*storage, *target, "StorageBorrowMut")? else {
                    return Ok(false);
                };

                let Some(selected_release) =
                    self.storage_call(*storage, *target, "StorageRelease")?
                else {
                    return Ok(false);
                };

                release = Some(selected_release);

                vec![(StorageCleanupProjectionKind::OwnedTarget(borrow), *target)]
            }
            TypeData::Tuple(elements) => {
                let mut children = Vec::with_capacity(elements.len());

                for (index, element) in elements.iter().enumerate() {
                    let Ok(index) = u32::try_from(index) else {
                        return Ok(false);
                    };

                    children.push((
                        StorageCleanupProjectionKind::Component(StorageProjection::TupleElement(
                            SymbolOrdinal::new(index),
                        )),
                        *element,
                    ));
                }

                children
            }
            TypeData::Nullable(element) => vec![(
                StorageCleanupProjectionKind::Component(StorageProjection::NullableValue),
                *element,
            )],
            TypeData::Array { element, length } => vec![(
                StorageCleanupProjectionKind::ArrayElements(*length),
                *element,
            )],
            TypeData::Named {
                definition,
                substitution,
            } => {
                let lifecycle = self.context.declared_type_has_lifecycle(*definition)?;

                self.diagnostics.add_range(lifecycle.diagnostics().clone());

                if lifecycle.diagnostics().has_errors() {
                    return Ok(false);
                }

                if expansion == CleanupExpansion::Completion && *lifecycle.value() {
                    return Ok(append_whole_part(cleanup, path, parts));
                }

                if expansion == CleanupExpansion::MovedPaths && *lifecycle.value() {
                    self.report_incomplete_lifecycle(ty, source)?;

                    return Ok(false);
                }

                requires_whole_value = *lifecycle.value();

                let Some(children) = self.named_components(*definition, *substitution)? else {
                    return Ok(false);
                };

                children
            }
            _ => return Ok(false),
        };

        self.cleanup_types.insert(
            ty,
            bray_bound_tree::StorageCleanupType::new(ty, cleanup).with_components(
                children.iter().map(|(projection, result)| {
                    StorageCleanupProjection::new(*projection, ty, *result)
                }),
                release,
                requires_whole_value,
            ),
        );

        if moved
            .iter()
            .filter_map(|path| path.first())
            .any(|projection| {
                !children
                    .iter()
                    .any(|(child, _)| child.contains(*projection))
            })
        {
            return Ok(false);
        }

        for (projection, child_type) in children.into_iter().rev() {
            let child_moves = moved
                .iter()
                .filter_map(|path| {
                    path.split_first()
                        .filter(|(head, _)| projection.contains(**head))
                        .map(|(_, tail)| tail)
                })
                .collect::<Vec<_>>();

            path.push(StorageCleanupProjection::new(projection, ty, child_type));

            let complete = self.append_parts(
                child_type,
                &child_moves,
                if expansion == CleanupExpansion::Completion {
                    CleanupExpansion::Completion
                } else {
                    CleanupExpansion::MovedPaths
                },
                source,
                path,
                parts,
            )?;

            path.pop();

            if !complete {
                return Ok(false);
            }
        }

        if let Some(release) = release {
            parts.push(StorageCleanupPart::release_storage(
                path.iter().copied(),
                release,
            ));
        }

        Ok(true)
    }

    fn named_components(
        &mut self,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<
        Option<Vec<(StorageCleanupProjectionKind, TypeId)>>,
        CheckerQueryError<C::UpstreamError>,
    > {
        let representation = self.context.declared_type_representation(definition)?;

        self.diagnostics
            .add_range(representation.diagnostics().clone());

        if representation.value().is_recovered() {
            return Ok(None);
        }

        let mut children = Vec::new();

        match representation.value().storage() {
            DeclaredStorageShape::Structure(members) => {
                for (index, member) in members.iter().enumerate() {
                    let projection = match member.field() {
                        Some(field) => StorageProjection::ProductField(field),
                        None => {
                            let Ok(index) = u32::try_from(index) else {
                                return Ok(None);
                            };

                            StorageProjection::TupleElement(SymbolOrdinal::new(index))
                        }
                    };

                    let Some(ty) = self.member_type(member.ty(), substitution)? else {
                        return Ok(None);
                    };

                    children.push((StorageCleanupProjectionKind::Component(projection), ty));
                }
            }
            DeclaredStorageShape::Union(variants) => {
                for variant in variants.iter() {
                    for (index, member) in variant.members().iter().enumerate() {
                        let projection = match member.field() {
                            Some(field) => StorageCleanupProjectionKind::Component(
                                StorageProjection::ActiveUnionPayloadField {
                                    variant: variant.variant(),
                                    field,
                                },
                            ),
                            None => {
                                let Ok(index) = u32::try_from(index) else {
                                    return Ok(None);
                                };

                                StorageCleanupProjectionKind::UnionPayloadElement {
                                    variant: variant.variant(),
                                    ordinal: SymbolOrdinal::new(index),
                                }
                            }
                        };

                        let Some(ty) = self.member_type(member.ty(), substitution)? else {
                            return Ok(None);
                        };

                        children.push((projection, ty));
                    }
                }
            }
        }

        Ok(Some(children))
    }

    fn storage_call(
        &mut self,
        storage: TypeId,
        target: TypeId,
        member: &str,
    ) -> Result<Option<StorageProtocolCall>, CheckerQueryError<C::UpstreamError>> {
        let Some(member) = bray_compiler_known::CompilerKnownDeclarationKey::try_new(member) else {
            return Ok(None);
        };

        let selected =
            crate::storage::selected_storage_protocol_call(self.context, storage, target, &member)?;

        let (call, diagnostics) = selected.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(call)
    }
}

fn append_whole_part(
    cleanup: AsyncStorageCleanupRequirement,
    path: &[StorageCleanupProjection],
    parts: &mut Vec<StorageCleanupPart>,
) -> bool {
    match cleanup {
        AsyncStorageCleanupRequirement::None => true,
        AsyncStorageCleanupRequirement::Cleanup(phases) => {
            parts.push(StorageCleanupPart::new(path.iter().copied(), phases));

            true
        }
        AsyncStorageCleanupRequirement::Recovered(_) => false,
    }
}
