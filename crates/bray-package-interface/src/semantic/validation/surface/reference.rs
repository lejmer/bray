use bray_symbols::SymbolKind;

use crate::{InterfaceSymbolReference, InterfaceValidationError, PackageInterfaceSurface};

pub(in crate::semantic::validation) fn validate_owned_parameter(
    owner: &InterfaceSymbolReference,
    parameter: &InterfaceSymbolReference,
    expected_kind: SymbolKind,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    if validate_symbol_kind(parameter, surface)? != expected_kind
        || !reference_is_owned_by(parameter, owner, surface)?
    {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(())
}

pub(super) fn reference_is_owned_by(
    member: &InterfaceSymbolReference,
    owner: &InterfaceSymbolReference,
    surface: &PackageInterfaceSurface,
) -> Result<bool, InterfaceValidationError> {
    match (member, owner) {
        (
            InterfaceSymbolReference::CompilerKnown(_),
            InterfaceSymbolReference::CompilerKnown(_),
        ) => Ok(true),
        (InterfaceSymbolReference::CompilerKnown(_), _)
        | (_, InterfaceSymbolReference::CompilerKnown(_)) => Ok(false),
        _ => Ok(reference_owner(member, surface)? == Some(reference_key(owner, surface)?)),
    }
}

pub(in crate::semantic::validation) fn relationship_members(
    surface: &PackageInterfaceSurface,
    owner: bray_symbols::InterfaceSymbolId,
    relationship_kind: bray_symbols::SymbolRelationshipKind,
    member_kind: SymbolKind,
) -> Vec<InterfaceSymbolReference> {
    surface
        .relationships()
        .iter()
        .filter(|relationship| {
            relationship.kind() == relationship_kind && relationship.owner() == owner
        })
        .filter_map(|relationship| {
            let member = surface.symbols().symbol(relationship.member())?;

            (member.kind() == member_kind)
                .then_some(InterfaceSymbolReference::Local(relationship.member()))
        })
        .collect::<Vec<_>>()
}

pub(super) fn reference_owner<'surface>(
    reference: &'surface InterfaceSymbolReference,
    surface: &'surface PackageInterfaceSurface,
) -> Result<Option<&'surface bray_symbols::ExternalSymbolKey>, InterfaceValidationError> {
    Ok(reference_key(reference, surface)?.owner())
}

pub(super) fn reference_key<'surface>(
    reference: &'surface InterfaceSymbolReference,
    surface: &'surface PackageInterfaceSurface,
) -> Result<&'surface bray_symbols::ExternalSymbolKey, InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(id) => surface
            .symbols()
            .symbol(*id)
            .map(|symbol| symbol.key())
            .ok_or(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            )),
        InterfaceSymbolReference::Dependency { key, .. } => Ok(key),
        InterfaceSymbolReference::CompilerKnown(_) => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        )),
    }
}

pub(in crate::semantic::validation) fn local_symbol(
    reference: &InterfaceSymbolReference,
) -> Result<bray_symbols::InterfaceSymbolId, InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(symbol) => Ok(*symbol),
        InterfaceSymbolReference::Dependency { .. }
        | InterfaceSymbolReference::CompilerKnown(_) => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        )),
    }
}

pub(in crate::semantic::validation) fn validate_symbol(
    reference: &InterfaceSymbolReference,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(id) => validate_index(id.to_index(), symbol_count),
        InterfaceSymbolReference::Dependency { dependency, .. } => {
            validate_index(dependency.to_index(), dependency_count)
        }
        InterfaceSymbolReference::CompilerKnown(_) => Ok(()),
    }
}

pub(in crate::semantic::validation) fn validate_symbol_kind(
    reference: &InterfaceSymbolReference,
    surface: &PackageInterfaceSurface,
) -> Result<SymbolKind, InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(id) => surface
            .symbols()
            .symbol(*id)
            .map(|symbol| symbol.kind())
            .ok_or(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            )),
        InterfaceSymbolReference::Dependency { dependency, key } => {
            let Some(dependency) = dependency
                .to_index()
                .and_then(|index| surface.dependencies().get(index))
            else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            };

            if key.package_identity() != dependency.package() {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }

            Ok(key.kind())
        }
        InterfaceSymbolReference::CompilerKnown(reference) => Ok(reference.kind()),
    }
}

pub(in crate::semantic::validation) fn validate_index(
    index: Option<usize>,
    length: usize,
) -> Result<(), InterfaceValidationError> {
    super::super::checked_index(index, length).map(|_| ())
}
