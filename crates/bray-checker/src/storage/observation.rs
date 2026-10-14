use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{StorageInvalidation, StoragePath, StoragePlan, StorageProjection};
use bray_symbols::{DeclaredStorageShape, NamedTypeSymbolId, TypeData, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Observation {
    Mutable,
    Scalar,
    Address,
    Identity,
    Discriminant,
}

pub(crate) struct Footprint {
    pub(crate) atomic: StorageInvalidation,
    pub(crate) scalars: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) borrowed_identities: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) identities: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) discriminants: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
}

/// Observations that may include protected atomic payloads, including unknown raw reachability.
/// Ordinary fields remain live: an atomic update neither replaces nor retires their owner.
pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<Footprint, CheckerQueryError<C::UpstreamError>> {
    let mut accesses = BTreeMap::new();
    let mut shapes = BTreeMap::new();
    let mut stable_descriptors = BTreeSet::new();
    let mut identities = BTreeMap::new();
    let mut scalars = BTreeMap::new();
    let mut discriminants = BTreeMap::new();
    let mut borrowed_identities = BTreeMap::new();
    let mut static_types = BTreeMap::<_, BTreeSet<_>>::new();

    for (_, access) in storage.access_entries() {
        if let bray_bound_tree::StorageAccessRoot::Storage(root) = access.root()
            && access.projections().is_empty()
            && matches!(
                storage.identity(root),
                Some(bray_bound_tree::StorageIdentity::Static(_))
            )
        {
            static_types
                .entry(root)
                .or_default()
                .insert(access.reached_type());
        }
    }

    for (root, identity) in storage.identity_entries() {
        let Some(access) = storage.root_access(root) else {
            continue;
        };

        let Some(ty) = storage.storage_type(root) else {
            accesses.insert(access, vec![StoragePath::root(root)]);
            continue;
        };

        let shared = match request.semantic_values().type_data(ty).as_ref() {
            TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Shared,
                target,
            } => Some(*target),
            _ => None,
        };

        let ty = shared.unwrap_or(ty);

        if !shapes.contains_key(&ty) {
            shapes.insert(ty, paths(request, ty, &mut BTreeSet::new())?);
        }

        let mut observations = shapes[&ty].iter().cloned().collect::<BTreeSet<_>>();

        // Aliasing groups static specializations by declaration. Preserve a marker
        // only when every selected storage shape proves that same observation.
        for ty in static_types.get(&root).into_iter().flatten() {
            if !shapes.contains_key(ty) {
                shapes.insert(*ty, paths(request, *ty, &mut BTreeSet::new())?);
            }

            observations.retain(|observation| {
                observation.1 != Observation::Identity || shapes[ty].contains(observation)
            });

            observations.extend(
                shapes[ty]
                    .iter()
                    .filter(|(_, observation)| *observation != Observation::Identity)
                    .cloned(),
            );
        }

        let paths: Vec<_> = observations
            .iter()
            .filter_map(|(projections, observation)| {
                let path = StoragePath {
                    root,
                    projections: projections.clone().into(),
                };

                match observation {
                    Observation::Identity => {
                        if shared.is_some() {
                            borrowed_identities
                                .entry(root)
                                .or_insert_with(Vec::new)
                                .push(path);
                        } else if identity.is_invocation_owned(storage.kind())
                            || matches!(identity, bray_bound_tree::StorageIdentity::Static(_))
                        {
                            identities.entry(root).or_insert_with(Vec::new).push(path);
                        }

                        None
                    }
                    Observation::Discriminant => {
                        if shared.is_none() && identity.is_invocation_owned(storage.kind()) {
                            discriminants
                                .entry(root)
                                .or_insert_with(Vec::new)
                                .push(path);
                        }

                        None
                    }
                    Observation::Scalar => {
                        if shared.is_none() && identity.is_invocation_owned(storage.kind()) {
                            scalars
                                .entry(root)
                                .or_insert_with(Vec::new)
                                .push(path.clone());
                        }

                        Some(path)
                    }
                    Observation::Address => {
                        stable_descriptors.insert(path.clone());

                        Some(path)
                    }
                    Observation::Mutable => Some(path),
                }
            })
            .collect();

        if !paths.is_empty() {
            accesses.insert(access, paths);
        }
    }

    Ok(Footprint {
        atomic: StorageInvalidation::Observed {
            accesses,
            snapshots: BTreeMap::new(),
            referents: BTreeSet::new(),
            stable_descriptors,
        },
        identities,
        scalars,
        borrowed_identities,
        discriminants,
    })
}

fn paths<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    visiting: &mut BTreeSet<NamedTypeSymbolId>,
) -> Result<Vec<(Vec<StorageProjection>, Observation)>, CheckerQueryError<C::UpstreamError>> {
    if request.is_cancelled() {
        return Err(CheckerQueryError::Cancelled);
    }

    let reference = matches!(
        request.semantic_values().type_data(ty).as_ref(),
        TypeData::Borrow { .. }
    );

    let borrowed = matches!(
        request.semantic_values().type_data(ty).as_ref(),
        TypeData::Borrow {
            kind: bray_symbols::BorrowKind::Mutable,
            ..
        }
    );

    let ty = request.semantic_values().unborrowed_type(ty);
    let data = request.semantic_values().type_data(ty);
    let mut fields = Vec::new();

    let definition = match data.as_ref() {
        TypeData::Named {
            definition,
            substitution,
        } => {
            let symbols = request.available_compiler_known_symbols();

            let role = match definition {
                NamedTypeSymbolId::Struct(id) => symbols.symbol_representation(*id),
                NamedTypeSymbolId::Union(id) => symbols.symbol_representation(*id),
            };

            if let Some(role) = role.filter(|role| {
                !matches!(
                    role,
                    bray_compiler_known::RepresentationRole::Result
                        | bray_compiler_known::RepresentationRole::RunResult
                )
            }) {
                if matches!(
                    role,
                    bray_compiler_known::RepresentationRole::Unit
                        | bray_compiler_known::RepresentationRole::Never
                ) {
                    return Ok(Vec::new());
                }

                if role.numeric_kind().is_some()
                    || matches!(
                        role,
                        bray_compiler_known::RepresentationRole::ScalarBool
                            | bray_compiler_known::RepresentationRole::ScalarChar
                    )
                {
                    return Ok(vec![(
                        Vec::new(),
                        if reference {
                            Observation::Mutable
                        } else {
                            Observation::Scalar
                        },
                    )]);
                }

                return Ok(vec![(
                    Vec::new(),
                    if matches!(
                        role,
                        bray_compiler_known::RepresentationRole::RawPointer
                            | bray_compiler_known::RepresentationRole::DevicePointer
                    ) {
                        Observation::Address
                    } else {
                        Observation::Mutable
                    },
                )]);
            }

            // A recursive reference widens only that component, not its siblings.
            if !visiting.insert(*definition) {
                return Ok(vec![(Vec::new(), Observation::Mutable)]);
            }

            let representation = request.declared_type_representation(*definition)?;

            let members: Vec<_> = match representation.value().storage() {
                DeclaredStorageShape::Structure(members) => members
                    .iter()
                    .map(|member| {
                        (
                            member.field().map(StorageProjection::ProductField),
                            member.ty(),
                        )
                    })
                    .collect(),
                DeclaredStorageShape::Union(variants) => variants
                    .iter()
                    .flat_map(|variant| {
                        variant.members().iter().map(move |member| {
                            (
                                member.field().map(|field| {
                                    StorageProjection::ActiveUnionPayloadField {
                                        variant: variant.variant(),
                                        field,
                                    }
                                }),
                                member.ty(),
                            )
                        })
                    })
                    .collect(),
            };

            for (projection, template) in members {
                let Some(projection) = projection else {
                    visiting.remove(definition);

                    return Ok(vec![(Vec::new(), Observation::Mutable)]);
                };

                let checked =
                    crate::constant::checked_substituted_type(request, template, *substitution)?;

                let Some(ty) = *checked.value() else {
                    visiting.remove(definition);

                    return Ok(vec![(Vec::new(), Observation::Mutable)]);
                };

                fields.push((projection, ty));
            }

            Some(*definition)
        }
        TypeData::Nullable(element) => {
            fields.push((StorageProjection::NullableValue, *element));

            None
        }
        TypeData::Tuple(elements) => {
            fields.extend(elements.iter().enumerate().map(|(index, ty)| {
                (
                    StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(
                        index
                            .try_into()
                            .expect("tuple ordinal fits its checked type"),
                    )),
                    *ty,
                )
            }));

            None
        }
        _ => return Ok(vec![(Vec::new(), Observation::Mutable)]),
    };

    let mut result = if matches!(definition, Some(NamedTypeSymbolId::Struct(_)))
        && fields.is_empty()
        && !borrowed
    {
        vec![(Vec::new(), Observation::Identity)]
    } else {
        Vec::new()
    };

    if matches!(
        data.as_ref(),
        TypeData::Nullable(_)
            | TypeData::Named {
                definition: NamedTypeSymbolId::Union(_),
                ..
            }
    ) {
        result.push((Vec::new(), Observation::Discriminant));
    }

    for (field, ty) in fields {
        result.extend(
            paths(request, ty, visiting)?
                .into_iter()
                .map(|(path, descriptor)| {
                    (std::iter::once(field).chain(path).collect(), descriptor)
                }),
        );
    }

    if reference {
        result.retain(|(_, observation)| *observation != Observation::Scalar);
    }

    if borrowed {
        result.retain(|(_, observation)| {
            !matches!(
                observation,
                Observation::Identity | Observation::Discriminant
            )
        });
    }

    if let Some(definition) = definition {
        visiting.remove(&definition);
    }

    Ok(result)
}

/// Intersects physical may-alias observations with the atomic operation's closed write domain.
pub(crate) fn restrict(
    invalidation: StorageInvalidation,
    frame: &StorageInvalidation,
) -> StorageInvalidation {
    let StorageInvalidation::Observed {
        accesses: frame_paths,
        stable_descriptors: descriptors,
        ..
    } = frame
    else {
        unreachable!("prepared atomic frame contains resolved observations");
    };

    let (mut accesses, mut referents, mut snapshots, mut stable_descriptors) = match invalidation {
        StorageInvalidation::Observed {
            accesses,
            referents,
            snapshots,
            stable_descriptors,
        } => (accesses, referents, snapshots, stable_descriptors),
        StorageInvalidation::All {
            stable_values,
            mut stable_descriptors,
        } => {
            let mut accesses = frame_paths.clone();

            accesses.retain(|_, paths| {
                paths.retain(|path| !stable_values.iter().any(|stable| stable.contains(path)));

                !paths.is_empty()
            });

            stable_descriptors.extend(descriptors.iter().cloned());

            return StorageInvalidation::Observed {
                accesses,
                referents: BTreeSet::new(),
                snapshots: BTreeMap::new(),
                stable_descriptors,
            };
        }
        StorageInvalidation::Paths(_) => return frame.clone(),
    };

    let intersect = |path: &StoragePath| {
        frame_paths
            .values()
            .flatten()
            .filter_map(|candidate| {
                if path.contains(candidate) {
                    Some(candidate.clone())
                } else if candidate.contains(path) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>()
    };

    accesses.retain(|_, paths| {
        *paths = paths
            .iter()
            .flat_map(&intersect)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        !paths.is_empty()
    });

    referents = referents.iter().flat_map(intersect).collect();
    snapshots.retain(|path, _| accesses.values().flatten().any(|changed| changed == path));
    stable_descriptors.extend(descriptors.iter().cloned());

    StorageInvalidation::Observed {
        accesses,
        referents,
        snapshots,
        stable_descriptors,
    }
}
