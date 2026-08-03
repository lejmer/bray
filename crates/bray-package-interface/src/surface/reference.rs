use std::sync::Arc;

use bray_symbols::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ImportedSymbolIdentityInput, InterfaceSymbolId,
    ModulePathKey, SymbolKey, SymbolKind, SymbolOrdinal, SynthesizedSymbolKey,
    SynthesizedSymbolRole,
};

use super::decoding::{package_identity, read_string, read_tag, symbol_name};
use super::model::MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS;
use crate::InterfaceValidationError;
use crate::decode::{DecodeBudget, read_optional_u32, read_u32};
use crate::wire::WireReader;

pub(super) fn decode_local_key_component(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    kind: SymbolKind,
    container: Option<InterfaceSymbolId>,
    symbols: &[ImportedSymbolIdentityInput],
    budget: &mut DecodeBudget,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let shape = read_u32(reader)?;

    match shape {
        1 => decode_package_key(reader, strings, kind, container),
        2 => {
            if kind != SymbolKind::Module {
                return Err(InterfaceValidationError::Malformed);
            }

            ExternalSymbolKey::module(
                local_owner(container, symbols)?,
                decode_module_path(reader, strings, budget)?,
            )
            .ok_or(InterfaceValidationError::Malformed)
        }
        3 => decode_declaration_key(reader, strings, local_owner(container, symbols)?, kind),
        4 => decode_synthesized_key(reader, local_owner(container, symbols)?, kind),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_external_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let count =
        usize::try_from(read_u32(reader)?).map_err(|_| InterfaceValidationError::Malformed)?;

    if count == 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    budget.charge_external_reference(count)?;
    budget.charge_items::<ExternalSymbolKey>(count)?;

    let mut key = None;

    for _ in 0..count {
        let kind: SymbolKind = read_tag(reader)?;
        let shape = read_u32(reader)?;

        key = Some(decode_external_key_component(
            reader, strings, key, kind, shape, budget,
        )?);
    }

    key.ok_or(InterfaceValidationError::Malformed)
}

pub(super) fn decode_compiler_known_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<SymbolKey, InterfaceValidationError> {
    let count =
        usize::try_from(read_u32(reader)?).map_err(|_| InterfaceValidationError::Malformed)?;

    if count == 0 || count > MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS {
        return Err(InterfaceValidationError::Malformed);
    }

    budget.charge_external_reference(count)?;
    budget.charge_items::<SymbolKey>(count)?;

    let mut key = None;

    for index in 0..count {
        key = Some(match read_u32(reader)? {
            1 if index == 0 => {
                let declaration = bray_compiler_known::CompilerKnownDeclarationKey::try_new(
                    Arc::clone(read_string(reader, strings)?),
                )
                .ok_or(InterfaceValidationError::Malformed)?;

                let kind = read_tag(reader)?;

                SymbolKey::compiler_known_declaration(declaration, kind)
                    .ok_or(InterfaceValidationError::Malformed)?
            }
            2 if index > 0 => {
                let subject = key.ok_or(InterfaceValidationError::Malformed)?;
                let role: SynthesizedSymbolRole = read_tag(reader)?;
                let ordinal = read_optional_u32(reader)?.map(SymbolOrdinal::new);

                let synthesized = SynthesizedSymbolKey::try_new(role, subject, ordinal)
                    .ok_or(InterfaceValidationError::Malformed)?;

                SymbolKey::synthesized(synthesized)
            }
            _ => return Err(InterfaceValidationError::Malformed),
        });
    }

    key.ok_or(InterfaceValidationError::Malformed)
}

fn decode_external_key_component(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    owner: Option<ExternalSymbolKey>,
    kind: SymbolKind,
    shape: u32,
    budget: &mut DecodeBudget,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    match shape {
        1 if owner.is_none() => decode_package_key(reader, strings, kind, None),
        2 => ExternalSymbolKey::module(
            owner.ok_or(InterfaceValidationError::Malformed)?,
            decode_module_path(reader, strings, budget)?,
        )
        .ok_or(InterfaceValidationError::Malformed),
        3 => decode_declaration_key(
            reader,
            strings,
            owner.ok_or(InterfaceValidationError::Malformed)?,
            kind,
        ),
        4 => decode_synthesized_key(
            reader,
            owner.ok_or(InterfaceValidationError::Malformed)?,
            kind,
        ),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn decode_package_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    kind: SymbolKind,
    container: Option<InterfaceSymbolId>,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    if kind != SymbolKind::Package || container.is_some() {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(ExternalSymbolKey::package(package_identity(read_string(
        reader, strings,
    )?)?))
}

fn decode_declaration_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    owner: ExternalSymbolKey,
    kind: SymbolKind,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    match decode_declaration_identity(reader, strings)? {
        ExternalDeclarationIdentity::Name(name) => ExternalSymbolKey::named(owner, kind, name),
        ExternalDeclarationIdentity::Ordinal(ordinal) => {
            ExternalSymbolKey::ordinal(owner, kind, ordinal)
        }
    }
    .ok_or(InterfaceValidationError::Malformed)
}

fn decode_synthesized_key(
    reader: &mut WireReader<'_>,
    owner: ExternalSymbolKey,
    kind: SymbolKind,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let role: SynthesizedSymbolRole = read_tag(reader)?;
    let ordinal = read_optional_u32(reader)?.map(SymbolOrdinal::new);

    let key = ExternalSymbolKey::synthesized(owner, role, ordinal)
        .ok_or(InterfaceValidationError::Malformed)?;

    if key.kind() != kind {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(key)
}

fn decode_module_path(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<ModulePathKey, InterfaceValidationError> {
    let count =
        usize::try_from(read_u32(reader)?).map_err(|_| InterfaceValidationError::Malformed)?;

    let mut segments = budget.allocate_items(reader, count)?;

    // Module paths share the validated string table instead of allocating duplicate text.
    for _ in 0..count {
        segments.push(Arc::clone(read_string(reader, strings)?));
    }

    ModulePathKey::try_new(segments).ok_or(InterfaceValidationError::Malformed)
}

fn decode_declaration_identity(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
) -> Result<ExternalDeclarationIdentity, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(ExternalDeclarationIdentity::Name(symbol_name(
            read_string(reader, strings)?,
        )?)),
        2 => Ok(ExternalDeclarationIdentity::Ordinal(SymbolOrdinal::new(
            read_u32(reader)?,
        ))),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn local_owner(
    container: Option<InterfaceSymbolId>,
    symbols: &[ImportedSymbolIdentityInput],
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    // External keys are Arc-backed immutable identities and are cheap to share while decoding.
    container
        .and_then(InterfaceSymbolId::to_index)
        .and_then(|index| symbols.get(index))
        .map(|symbol| symbol.key().clone())
        .ok_or(InterfaceValidationError::Malformed)
}
