use std::collections::{BTreeMap, BTreeSet};

use crate::relationship::RelationshipIndex;
use crate::{
    AnySymbolId, ImportedSymbolIdentity, ImportedSymbolSkeleton, ImportedSymbolSkeletonBuildError,
    ImportedSymbolSkeletonInput, SymbolId, SymbolRelationshipKind,
};

#[derive(Clone)]
pub(super) struct AssignedIdentity {
    pub(super) interface: crate::ImportedInterfaceId,
    pub(super) identity: ImportedSymbolIdentity,
    pub(super) id: AnySymbolId,
}

struct AssignedSymbols {
    identities: Vec<AssignedIdentity>,
    local_index: LocalSymbolIndex,
    external_index: BTreeMap<crate::ExternalSymbolKey, AnySymbolId>,
}

pub(super) fn build_imported_symbol_skeleton(
    first_symbol_id: SymbolId,
    inputs: impl IntoIterator<Item = ImportedSymbolSkeletonInput>,
) -> Result<ImportedSymbolSkeleton, ImportedSymbolSkeletonBuildError> {
    let mut inputs: Vec<_> = inputs.into_iter().collect();

    validate_and_sort_inputs(&mut inputs)?;

    let assigned = assign_symbol_ids(first_symbol_id, &inputs)?;
    let containers = remap_containers(&assigned.identities, &assigned.local_index)?;

    let (relationship_index, provider_subjects) =
        build_relationships(&inputs, &assigned.local_index, &containers)?;

    let lookups = build_lookups(&inputs, &assigned.local_index, &assigned.external_index)?;

    let records = super::records::build_records(
        assigned.identities,
        &containers,
        &provider_subjects,
        &relationship_index,
    )?;

    Ok(records.finish(assigned.external_index, lookups, &relationship_index))
}

fn validate_and_sort_inputs(
    inputs: &mut [ImportedSymbolSkeletonInput],
) -> Result<(), ImportedSymbolSkeletonBuildError> {
    inputs.sort_by(|left, right| left.symbols().package().cmp(right.symbols().package()));

    let mut interfaces = BTreeSet::new();
    let mut packages = BTreeSet::new();

    for input in inputs {
        if !interfaces.insert(input.interface()) {
            return Err(ImportedSymbolSkeletonBuildError::DuplicateInterface(
                input.interface(),
            ));
        }

        if !packages.insert(input.symbols().package()) {
            return Err(ImportedSymbolSkeletonBuildError::DuplicatePackage(
                input.symbols().package().clone(),
            ));
        }
    }

    Ok(())
}

type LocalSymbolIndex =
    BTreeMap<(crate::ImportedInterfaceId, crate::InterfaceSymbolId), AnySymbolId>;

fn assign_symbol_ids(
    first_symbol_id: SymbolId,
    inputs: &[ImportedSymbolSkeletonInput],
) -> Result<AssignedSymbols, ImportedSymbolSkeletonBuildError> {
    let mut identities = inputs
        .iter()
        .flat_map(|input| {
            input
                .symbols()
                .symbols()
                .iter()
                .cloned()
                .map(move |identity| (input.interface(), identity))
        })
        .collect::<Vec<_>>();

    identities.sort_by(|left, right| left.1.key().cmp(right.1.key()));

    let mut assigned = Vec::with_capacity(identities.len());

    let mut local_index = BTreeMap::new();
    let mut external_index = BTreeMap::new();

    for (offset, (interface, identity)) in identities.into_iter().enumerate() {
        let offset = u32::try_from(offset)
            .map_err(|_| ImportedSymbolSkeletonBuildError::SymbolIdOverflow)?;

        let raw = first_symbol_id
            .raw()
            .checked_add(offset)
            .ok_or(ImportedSymbolSkeletonBuildError::SymbolIdOverflow)?;

        let symbol_id = SymbolId::new(raw);

        let id = AnySymbolId::from_kind(identity.kind(), symbol_id).ok_or(
            ImportedSymbolSkeletonBuildError::UnsupportedSymbolKind(identity.kind()),
        )?;

        if external_index.insert(identity.key().clone(), id).is_some() {
            return Err(ImportedSymbolSkeletonBuildError::DuplicateExternalKey(
                identity.key().clone(),
            ));
        }

        local_index.insert((interface, identity.id()), id);

        assigned.push(AssignedIdentity {
            interface,
            identity,
            id,
        });
    }

    Ok(AssignedSymbols {
        identities: assigned,
        local_index,
        external_index,
    })
}

fn remap_containers(
    assigned: &[AssignedIdentity],
    local_index: &LocalSymbolIndex,
) -> Result<BTreeMap<AnySymbolId, Option<AnySymbolId>>, ImportedSymbolSkeletonBuildError> {
    assigned
        .iter()
        .map(|assigned| {
            let container = assigned
                .identity
                .container()
                .map(|container| local_symbol(local_index, assigned.interface, container, true))
                .transpose()?;

            Ok((assigned.id, container))
        })
        .collect()
}

fn build_relationships(
    inputs: &[ImportedSymbolSkeletonInput],
    local_index: &LocalSymbolIndex,
    containers: &BTreeMap<AnySymbolId, Option<AnySymbolId>>,
) -> Result<(RelationshipIndex, BTreeMap<AnySymbolId, AnySymbolId>), ImportedSymbolSkeletonBuildError>
{
    let mut relationships = inputs
        .iter()
        .flat_map(|input| {
            input
                .relationships()
                .iter()
                .copied()
                .map(move |relationship| (input.interface(), relationship))
        })
        .map(|(interface, relationship)| {
            let owner = local_symbol(local_index, interface, relationship.owner(), true)?;
            let member = local_symbol(local_index, interface, relationship.member(), true)?;

            Ok((relationship, owner, member))
        })
        .collect::<Result<Vec<_>, ImportedSymbolSkeletonBuildError>>()?;

    relationships.sort_by_key(|(relationship, owner, member)| {
        (relationship.kind(), *owner, relationship.ordinal(), *member)
    });

    let mut index = RelationshipIndex::default();
    let mut provider_subjects = BTreeMap::new();
    let mut contained = BTreeSet::new();
    let mut next_ordinals = BTreeMap::new();

    for (relationship, owner, member) in relationships {
        if !relationship.kind().supports(owner.kind(), member.kind()) {
            return Err(ImportedSymbolSkeletonBuildError::InvalidRelationshipKinds {
                relationship: relationship.kind(),
                owner: owner.kind(),
                member: member.kind(),
            });
        }

        let expected = next_ordinals
            .entry((relationship.kind(), owner))
            .or_insert(0);

        if relationship.ordinal() != *expected {
            return Err(
                ImportedSymbolSkeletonBuildError::NonCanonicalRelationshipOrdinal {
                    relationship: relationship.kind(),
                    owner,
                    expected: *expected,
                    actual: relationship.ordinal(),
                },
            );
        }

        *expected = expected
            .checked_add(1)
            .ok_or(ImportedSymbolSkeletonBuildError::SymbolIdOverflow)?;

        if relationship.allows_mutation() {
            if !matches!(
                member,
                AnySymbolId::StructField(_) | AnySymbolId::UnionPayloadField(_)
            ) {
                return Err(ImportedSymbolSkeletonBuildError::InvalidRelationshipKinds {
                    relationship: relationship.kind(),
                    owner: owner.kind(),
                    member: member.kind(),
                });
            }

            index.allow_mutation(member);
        }

        if relationship.position() == crate::CallablePosition::PositionalOrNamed {
            if !matches!(member, AnySymbolId::UnionPayloadField(_)) {
                return Err(ImportedSymbolSkeletonBuildError::InvalidRelationshipKinds {
                    relationship: relationship.kind(),
                    owner: owner.kind(),
                    member: member.kind(),
                });
            }

            index.allow_positional(member);
        }

        if relationship.kind() != SymbolRelationshipKind::OverloadArm {
            if containers.get(&member).copied().flatten() != Some(owner) {
                return Err(
                    ImportedSymbolSkeletonBuildError::RelationshipContainmentMismatch {
                        owner,
                        member,
                    },
                );
            }

            if !contained.insert(member) {
                return Err(ImportedSymbolSkeletonBuildError::DuplicateContainment(
                    member,
                ));
            }
        }

        match relationship.kind() {
            SymbolRelationshipKind::OverloadArm => {
                index.add_imported_overload_arm(owner, member);
            }
            SymbolRelationshipKind::DefaultProvider => {
                index.add_provider(owner, member);
                provider_subjects.insert(member, owner);
            }
            _ => index.add_symbol(member, owner),
        }
    }

    for (symbol, container) in containers {
        if container.is_some() && !contained.contains(symbol) {
            return Err(ImportedSymbolSkeletonBuildError::MissingContainment(
                *symbol,
            ));
        }
    }

    Ok((index, provider_subjects))
}

fn build_lookups(
    inputs: &[ImportedSymbolSkeletonInput],
    local_index: &LocalSymbolIndex,
    external_index: &BTreeMap<crate::ExternalSymbolKey, AnySymbolId>,
) -> Result<
    BTreeMap<AnySymbolId, BTreeMap<crate::SymbolName, AnySymbolId>>,
    ImportedSymbolSkeletonBuildError,
> {
    let mut lookups = BTreeMap::<_, BTreeMap<_, _>>::new();

    for input in inputs {
        for edge in input.lookups() {
            let owner = local_symbol(local_index, input.interface(), edge.owner(), false)?;

            if !matches!(owner, AnySymbolId::Package(_) | AnySymbolId::Module(_)) {
                return Err(ImportedSymbolSkeletonBuildError::InvalidLookupOwner(owner));
            }

            let Some(target) = external_index.get(edge.target()).copied() else {
                return Err(ImportedSymbolSkeletonBuildError::MissingLookupTarget(
                    edge.target().clone(),
                ));
            };

            if lookups
                .entry(owner)
                .or_default()
                .insert(edge.name().clone(), target)
                .is_some()
            {
                return Err(ImportedSymbolSkeletonBuildError::DuplicateLookupName {
                    owner,
                    name: edge.name().clone(),
                });
            }
        }
    }

    Ok(lookups)
}

fn local_symbol(
    local_index: &LocalSymbolIndex,
    interface: crate::ImportedInterfaceId,
    symbol: crate::InterfaceSymbolId,
    relationship: bool,
) -> Result<AnySymbolId, ImportedSymbolSkeletonBuildError> {
    let error = if relationship {
        ImportedSymbolSkeletonBuildError::RelationshipSymbolOutOfBounds { interface, symbol }
    } else {
        ImportedSymbolSkeletonBuildError::LookupOwnerOutOfBounds {
            interface,
            owner: symbol,
        }
    };

    local_index.get(&(interface, symbol)).copied().ok_or(error)
}

#[cfg(test)]
mod tests {
    use crate::{
        ImportedInterfaceId, ImportedSymbolRelationship, ImportedSymbolSkeleton,
        ImportedSymbolSkeletonBuildError, ImportedSymbolSkeletonInput, InterfaceSymbolId, SymbolId,
        SymbolRelationshipKind,
    };

    use super::super::test_support::{build_skeleton, interface_fixture};

    #[test]
    fn external_key_assignment_is_deterministic_under_interface_permutations() {
        let first = interface_fixture(7, "z.package", "zeta");
        let second = interface_fixture(4, "a.package", "alpha");

        let forward = build_skeleton([first.input.clone(), second.input.clone()]);
        let reverse = build_skeleton([second.input, first.input]);

        assert_eq!(forward, reverse);

        assert_eq!(
            forward.symbol_by_external_key(&first.package_key),
            reverse.symbol_by_external_key(&first.package_key)
        );

        assert_eq!(
            forward.symbol_by_external_key(&second.function_key),
            reverse.symbol_by_external_key(&second.function_key)
        );

        let function = forward
            .symbol_by_external_key(&first.function_key)
            .unwrap_or_else(|| panic!("fixture function must receive an imported identity"));

        let module = forward
            .symbol_by_external_key(&first.module_key)
            .unwrap_or_else(|| panic!("fixture module must receive an imported identity"));

        assert_eq!(
            forward.member_name(function).map(|name| name.as_str()),
            Some("zeta")
        );

        assert_eq!(
            forward.lookup_member(module, "zeta"),
            crate::MemberLookupResult::Found(function)
        );
    }

    #[test]
    fn out_of_bounds_relationship_symbols_are_rejected() {
        let fixture = interface_fixture(2, "example.package", "run");

        let malformed = ImportedSymbolSkeletonInput::new(
            ImportedInterfaceId::new(2),
            fixture.input.symbols().clone(),
            [ImportedSymbolRelationship::new(
                SymbolRelationshipKind::ModuleMember,
                InterfaceSymbolId::new(1),
                InterfaceSymbolId::new(99),
                0,
            )],
            [],
        );

        assert_eq!(
            ImportedSymbolSkeleton::try_new(SymbolId::new(0), [malformed]),
            Err(
                ImportedSymbolSkeletonBuildError::RelationshipSymbolOutOfBounds {
                    interface: ImportedInterfaceId::new(2),
                    symbol: InterfaceSymbolId::new(99),
                }
            )
        );
    }

    #[test]
    fn incompatible_relationship_kinds_are_rejected() {
        let fixture = interface_fixture(2, "example.package", "run");

        let malformed = ImportedSymbolSkeletonInput::new(
            ImportedInterfaceId::new(2),
            fixture.input.symbols().clone(),
            [ImportedSymbolRelationship::new(
                SymbolRelationshipKind::PackageModule,
                InterfaceSymbolId::new(0),
                InterfaceSymbolId::new(2),
                0,
            )],
            [],
        );

        let error = ImportedSymbolSkeleton::try_new(SymbolId::new(0), [malformed]);

        assert!(matches!(
            error,
            Err(ImportedSymbolSkeletonBuildError::InvalidRelationshipKinds {
                relationship: SymbolRelationshipKind::PackageModule,
                ..
            })
        ));
    }
}
