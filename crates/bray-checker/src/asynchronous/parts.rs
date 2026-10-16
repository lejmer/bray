use bray_bound_tree::{
    AsyncStorageCleanupRequirement, BoundSourceAnchor, StorageCleanupPart,
    StorageCleanupProjection, StorageCleanupProjectionKind, StorageProjection, StorageProtocolCall,
};
use bray_symbols::{DeclaredStorageShape, SymbolOrdinal, TypeData, TypeId};

use super::cleanup::{CleanupShapeResolver, cleanup_requirement};
use crate::{CheckerQueryError, CheckerRequestContext};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum CleanupExpansion {
    MovedPaths,
    DestructorReceiver,
    FullDisposal,
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

        if expansion == CleanupExpansion::FullDisposal {
            self.expanded_types.insert(ty);

            let definition = match self.request.semantic_values().type_data(ty).as_ref() {
                TypeData::Named { definition, .. } => Some(*definition),
                _ => None,
            };

            let repeated = path.iter().any(|projection| projection.source_type() == ty
                || definition.is_some_and(|definition|
                    matches!(self.request.semantic_values().type_data(projection.source_type()).as_ref(),
                        TypeData::Named { definition: previous, .. } if *previous == definition)));

            if repeated {
                self.recursive_cleanup = true;

                return Ok(true);
            }
        }

        if expansion == CleanupExpansion::FullDisposal
            && matches!(cleanup, AsyncStorageCleanupRequirement::None)
        {
            return Ok(true);
        }

        if expansion == CleanupExpansion::MovedPaths && moved.iter().all(|path| path.is_empty()) {
            return Ok(match cleanup {
                AsyncStorageCleanupRequirement::None => true,
                AsyncStorageCleanupRequirement::Cleanup(phases) => {
                    parts.push(StorageCleanupPart::new(path.iter().copied(), phases));

                    true
                }
                AsyncStorageCleanupRequirement::Recovered(_) => false,
            });
        }

        let data = self.request.semantic_values().type_data(ty);

        let mut release = None;
        let mut requires_whole_value = false;
        let mut destructor_owns_parts = false;

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
                let lifecycle = self.request.declared_type_has_lifecycle(*definition)?;

                self.diagnostics.add_range(lifecycle.diagnostics().clone());

                if lifecycle.diagnostics().has_errors() {
                    return Ok(false);
                }

                if expansion == CleanupExpansion::MovedPaths && *lifecycle.value() {
                    self.report_incomplete_lifecycle(ty, source)?;

                    return Ok(false);
                }

                requires_whole_value = *lifecycle.value();

                if expansion == CleanupExpansion::FullDisposal && requires_whole_value {
                    let destructor = self.request.context().lifecycle_callable(
                        ty,
                        bray_symbols::TypeAssociatedLifecycleSlot::Destructor,
                    )?;

                    self.diagnostics.add_range(destructor.diagnostics().clone());

                    if destructor.diagnostics().has_errors() {
                        return Ok(false);
                    }

                    destructor_owns_parts = destructor.value().is_some();

                    parts.push(StorageCleanupPart::new(
                        path.iter().copied(),
                        bray_bound_tree::AsyncCleanupPhases::Lifecycle,
                    ));
                }

                let Some(children) = self.named_cleanup_children(*definition, *substitution)?
                else {
                    return Ok(false);
                };

                children
            }
            _ if expansion == CleanupExpansion::FullDisposal => {
                if let AsyncStorageCleanupRequirement::Cleanup(phases) = cleanup {
                    parts.push(StorageCleanupPart::new(path.iter().copied(), phases));

                    return Ok(true);
                }

                return Ok(false);
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

        // A declared consuming destructor includes its represented-part cleanup.
        // Its checked summary already covers those transitions on every outcome.
        // Expand children only when this invocation owns their cleanup itself.
        if destructor_owns_parts {
            return Ok(true);
        }

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
                if expansion == CleanupExpansion::FullDisposal {
                    expansion
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

    fn named_cleanup_children(
        &mut self,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<
        Option<Vec<(StorageCleanupProjectionKind, TypeId)>>,
        CheckerQueryError<C::UpstreamError>,
    > {
        let representation = self.request.declared_type_representation(definition)?;

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
            crate::storage::selected_storage_protocol_call(self.request, storage, target, &member)?;

        let (call, diagnostics) = selected.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(call)
    }
}

impl<C: CheckerRequestContext + ?Sized> CleanupShapeResolver<'_, C> {
    pub(super) fn completion_requirements(
        &mut self,
        ty: TypeId,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let parts = self.full_disposal(ty, self.request.unit().key().source())?;
        let mut requirements = std::collections::BTreeSet::new();

        for (ty, part) in parts.iter() {
            if let Some(release) = part.as_ref().and_then(|part| part.release()) {
                self.include_completion_requirements(release.callable_type(), &mut requirements);
                continue;
            }

            for slot in [
                bray_symbols::TypeAssociatedLifecycleSlot::Finalizer,
                bray_symbols::TypeAssociatedLifecycleSlot::Destructor,
            ] {
                let selected = self.request.context().lifecycle_callable(*ty, slot)?;

                self.diagnostics
                    .add_range(selected.diagnostics().iter().cloned());

                if let Some((_, signature)) = selected.value() {
                    self.include_completion_requirements(
                        signature.callable_type(),
                        &mut requirements,
                    );
                }
            }
        }

        let cleanup = self
            .cleanup_types
            .remove(&ty)
            .expect("completed result retains its cleanup shape");

        self.cleanup_types
            .insert(ty, cleanup.with_execution_requirements(requirements));

        Ok(())
    }

    fn include_completion_requirements(
        &self,
        callable_type: TypeId,
        requirements: &mut std::collections::BTreeSet<bray_symbols::CallableExecutionRequirement>,
    ) {
        let data = self.request.semantic_values().type_data(callable_type);

        let TypeData::Callable(callable) = data.as_ref() else {
            panic!("selected cleanup has a checked callable type");
        };

        requirements.extend(
            callable
                .phase_behaviors()
                .invocation()
                .execution_requirements()
                .iter()
                .copied(),
        );

        // Declared source requirements are available before body-effect inference.
        // Imported callable signatures already include their phase requirements.
        if callable.execution() == bray_symbols::CallableExecution::Synchronous {
            for condition in callable
                .phase_behaviors()
                .invocation()
                .predicate_requirements()
                .iter()
                .filter_map(|predicate| predicate.condition())
            {
                let term = self.request.semantic_values().constant_term_data(condition);

                let bray_symbols::ConstantTermData::PredicateCall { predicate, .. } = term.as_ref()
                else {
                    continue;
                };

                let declaration = predicate.definition().into_any();

                if self
                    .request
                    .available_compiler_known_symbols()
                    .symbol_implementation(declaration)
                    .is_some_and(|hook| hook.is_execution_requirement())
                {
                    requirements
                        .insert(bray_symbols::CallableExecutionRequirement::new(declaration));
                }
            }
        }

        // Deferred lifecycle execution is dispatched in its own described child frame.
    }

    fn full_disposal(
        &mut self,
        ty: TypeId,
        source: BoundSourceAnchor,
    ) -> Result<
        std::sync::Arc<[(TypeId, Option<StorageCleanupPart>)]>,
        CheckerQueryError<C::UpstreamError>,
    > {
        if let Some(parts) = self.full_parts.get(&ty) {
            return Ok(std::sync::Arc::clone(parts));
        }

        self.recursive_cleanup = false;
        self.expanded_types.clear();

        let parts = self.represented_parts(ty, &[], CleanupExpansion::FullDisposal, source)?;

        let mut receivers = parts
            .into_iter()
            .flatten()
            .map(|part| {
                (
                    part.projections()
                        .last()
                        .map_or(ty, |projection| projection.result_type()),
                    Some(part),
                )
            })
            .collect::<Vec<_>>();

        if self.recursive_cleanup {
            // A recursive subtree has no finite source access path. Its selected declarations still
            // incur their obligations, but a concrete parent's field evidence cannot prove them.
            receivers.extend(self.expanded_types.iter().copied().map(|ty| (ty, None)));
        }

        let receivers: std::sync::Arc<[_]> = receivers.into();

        self.full_parts
            .insert(ty, std::sync::Arc::clone(&receivers));

        Ok(receivers)
    }
}

pub(crate) fn full_cleanup_parts<C: CheckerRequestContext + ?Sized>(
    resolver: &mut CleanupShapeResolver<'_, C>,
    ty: TypeId,
    source: BoundSourceAnchor,
    destructor_receiver: bool,
) -> Result<
    bray_diagnostics::DiagnosticResult<Vec<(TypeId, Option<StorageCleanupPart>)>>,
    CheckerQueryError<C::UpstreamError>,
> {
    let mut receivers = Vec::new();

    let selected = if destructor_receiver {
        resolver.represented_parts(ty, &[], CleanupExpansion::DestructorReceiver, source)?
    } else {
        None
    };

    if let Some(selected) = selected.as_deref() {
        for part in selected {
            let reached = part
                .projections()
                .last()
                .map_or(ty, |projection| projection.result_type());

            if part.release().is_some() || !part.phases().includes_lifecycle() {
                // This occurrence owns the selected projection while the published plan retains it.
                receivers.push((reached, Some(part.clone())));

                continue;
            }

            let nested = resolver.full_disposal(reached, source)?;

            receivers.extend(nested.iter().map(|(ty, nested)| {
                let part = nested.as_ref().map(|nested| {
                    let projections = part
                        .projections()
                        .iter()
                        .copied()
                        .chain(nested.projections().iter().copied());

                    match nested.release() {
                        Some(release) => StorageCleanupPart::release_storage(projections, release),
                        None => StorageCleanupPart::new(projections, nested.phases()),
                    }
                });

                (*ty, part)
            }));
        }
    } else {
        // Each storage access owns its paths, while type expansion is shared across accesses.
        receivers.extend(resolver.full_disposal(ty, source)?.iter().cloned());
    }

    Ok(bray_diagnostics::DiagnosticResult::new(
        receivers,
        std::mem::take(&mut resolver.diagnostics),
    ))
}
