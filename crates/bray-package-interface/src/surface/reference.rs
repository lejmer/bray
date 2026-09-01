use std::sync::Arc;

use bray_symbols::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ImportedSymbolIdentityInput, InterfaceSymbolId,
    ModulePathKey, SymbolKey, SymbolKind, SymbolOrdinal, SynthesizedSymbolKey,
    SynthesizedSymbolRole,
};

use super::decoding::{
    invalid_discriminant, invalid_value, malformed, package_identity, read_string, read_tag,
    symbol_name,
};
use super::model::MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS;
use crate::decode::{DecodeBudget, read_optional_u32, read_u32};
use crate::wire::WireReader;
use crate::{
    InterfaceIntegerTarget, InterfaceMalformedCause, InterfaceValidationContext,
    InterfaceValidationError, InterfaceValidationField,
};

pub(super) fn decode_local_key_component(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    kind: SymbolKind,
    container: Option<InterfaceSymbolId>,
    symbols: &[ImportedSymbolIdentityInput],
    budget: &mut DecodeBudget,
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let shape = read_u32(reader, context, InterfaceValidationField::Discriminant)?;

    match shape {
        1 => decode_package_key(reader, strings, kind, container, context),
        2 => {
            if kind != SymbolKind::Module {
                return Err(invalid_value(context, InterfaceValidationField::SymbolKind));
            }

            ExternalSymbolKey::module(
                local_owner(container, symbols, context)?,
                decode_module_path(reader, strings, budget, context)?,
            )
            .ok_or_else(|| invalid_value(context, InterfaceValidationField::Identity))
        }
        3 => decode_declaration_key(
            reader,
            strings,
            local_owner(container, symbols, context)?,
            kind,
            context,
        ),
        4 => decode_synthesized_key(
            reader,
            local_owner(container, symbols, context)?,
            kind,
            context,
        ),
        _ => Err(invalid_discriminant(
            context,
            InterfaceValidationField::Discriminant,
            shape,
        )),
    }
}

pub(super) fn decode_external_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let raw_count = read_u32(reader, context, InterfaceValidationField::RecordCount)?;
    let count = usize::try_from(raw_count).map_err(|_| {
        numeric_overflow(
            context,
            InterfaceValidationField::RecordCount,
            u64::from(raw_count),
        )
    })?;

    if count == 0 {
        return Err(malformed(
            context,
            InterfaceMalformedCause::Missing {
                field: InterfaceValidationField::Identity,
            },
        ));
    }

    budget.charge_external_reference(count)?;
    budget.charge_items::<ExternalSymbolKey>(count)?;

    let mut key = None;

    for index in 0..count {
        let component_context = InterfaceValidationContext::ExternalSymbolKey {
            component: index as u64,
        };
        let kind: SymbolKind = read_tag(
            reader,
            component_context,
            InterfaceValidationField::SymbolKind,
        )?;
        let shape = read_u32(
            reader,
            component_context,
            InterfaceValidationField::Discriminant,
        )?;

        key = Some(decode_external_key_component(
            reader,
            strings,
            key,
            kind,
            shape,
            budget,
            component_context,
        )?);
    }

    key.ok_or_else(|| {
        malformed(
            context,
            InterfaceMalformedCause::Missing {
                field: InterfaceValidationField::Identity,
            },
        )
    })
}

pub(super) fn decode_compiler_known_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
    context: InterfaceValidationContext,
) -> Result<SymbolKey, InterfaceValidationError> {
    let raw_count = read_u32(reader, context, InterfaceValidationField::RecordCount)?;
    let count = usize::try_from(raw_count).map_err(|_| {
        numeric_overflow(
            context,
            InterfaceValidationField::RecordCount,
            u64::from(raw_count),
        )
    })?;

    if count == 0 || count > MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS {
        return Err(malformed(
            context,
            InterfaceMalformedCause::CountMismatch {
                field: InterfaceValidationField::RecordCount,
                expected: MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS as u64,
                actual: count as u64,
            },
        ));
    }

    budget.charge_external_reference(count)?;
    budget.charge_items::<SymbolKey>(count)?;

    let mut key = None;

    for index in 0..count {
        let component_context = InterfaceValidationContext::ExternalSymbolKey {
            component: index as u64,
        };
        let shape = read_u32(
            reader,
            component_context,
            InterfaceValidationField::Discriminant,
        )?;
        key = Some(match shape {
            1 if index == 0 => {
                let declaration = bray_compiler_known::CompilerKnownDeclarationKey::try_new(
                    Arc::clone(read_string(reader, strings, component_context)?),
                )
                .ok_or_else(|| {
                    invalid_value(component_context, InterfaceValidationField::Declaration)
                })?;

                let kind = read_tag(
                    reader,
                    component_context,
                    InterfaceValidationField::SymbolKind,
                )?;

                SymbolKey::compiler_known_declaration(declaration, kind).ok_or_else(|| {
                    invalid_value(component_context, InterfaceValidationField::Identity)
                })?
            }
            2 if index > 0 => {
                let subject = key.ok_or_else(|| {
                    malformed(
                        component_context,
                        InterfaceMalformedCause::Missing {
                            field: InterfaceValidationField::Subject,
                        },
                    )
                })?;
                let role: SynthesizedSymbolRole =
                    read_tag(reader, component_context, InterfaceValidationField::Role)?;
                let ordinal = read_optional_u32(
                    reader,
                    component_context,
                    InterfaceValidationField::Ordinal,
                )?
                .map(SymbolOrdinal::new);

                let synthesized = SynthesizedSymbolKey::try_new(role, subject, ordinal)
                    .ok_or_else(|| {
                        invalid_value(component_context, InterfaceValidationField::Identity)
                    })?;

                SymbolKey::synthesized(synthesized)
            }
            _ => {
                return Err(invalid_discriminant(
                    component_context,
                    InterfaceValidationField::Discriminant,
                    shape,
                ));
            }
        });
    }

    key.ok_or_else(|| {
        malformed(
            context,
            InterfaceMalformedCause::Missing {
                field: InterfaceValidationField::Identity,
            },
        )
    })
}

fn decode_external_key_component(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    owner: Option<ExternalSymbolKey>,
    kind: SymbolKind,
    shape: u32,
    budget: &mut DecodeBudget,
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    match shape {
        1 if owner.is_none() => decode_package_key(reader, strings, kind, None, context),
        2 => ExternalSymbolKey::module(
            owner.ok_or_else(|| {
                malformed(
                    context,
                    InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Owner,
                    },
                )
            })?,
            decode_module_path(reader, strings, budget, context)?,
        )
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::Identity)),
        3 => decode_declaration_key(
            reader,
            strings,
            owner.ok_or_else(|| {
                malformed(
                    context,
                    InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Owner,
                    },
                )
            })?,
            kind,
            context,
        ),
        4 => decode_synthesized_key(
            reader,
            owner.ok_or_else(|| {
                malformed(
                    context,
                    InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Owner,
                    },
                )
            })?,
            kind,
            context,
        ),
        _ => Err(invalid_discriminant(
            context,
            InterfaceValidationField::Discriminant,
            shape,
        )),
    }
}

fn decode_package_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    kind: SymbolKind,
    container: Option<InterfaceSymbolId>,
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    if kind != SymbolKind::Package || container.is_some() {
        return Err(invalid_value(context, InterfaceValidationField::Identity));
    }

    Ok(ExternalSymbolKey::package(package_identity(
        read_string(reader, strings, context)?,
        context,
    )?))
}

fn decode_declaration_key(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    owner: ExternalSymbolKey,
    kind: SymbolKind,
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    match decode_declaration_identity(reader, strings, context)? {
        ExternalDeclarationIdentity::Name(name) => ExternalSymbolKey::named(owner, kind, name),
        ExternalDeclarationIdentity::Ordinal(ordinal) => {
            ExternalSymbolKey::ordinal(owner, kind, ordinal)
        }
    }
    .ok_or_else(|| invalid_value(context, InterfaceValidationField::Identity))
}

fn decode_synthesized_key(
    reader: &mut WireReader<'_>,
    owner: ExternalSymbolKey,
    kind: SymbolKind,
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let role: SynthesizedSymbolRole = read_tag(reader, context, InterfaceValidationField::Role)?;
    let ordinal = read_optional_u32(reader, context, InterfaceValidationField::Ordinal)?
        .map(SymbolOrdinal::new);

    let key = ExternalSymbolKey::synthesized(owner, role, ordinal)
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::Identity))?;

    if key.kind() != kind {
        return Err(invalid_value(context, InterfaceValidationField::SymbolKind));
    }

    Ok(key)
}

fn decode_module_path(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
    context: InterfaceValidationContext,
) -> Result<ModulePathKey, InterfaceValidationError> {
    let raw_count = read_u32(reader, context, InterfaceValidationField::RecordCount)?;
    let count = usize::try_from(raw_count).map_err(|_| {
        numeric_overflow(
            context,
            InterfaceValidationField::RecordCount,
            u64::from(raw_count),
        )
    })?;

    let mut segments = budget.allocate_items(
        reader,
        context,
        InterfaceValidationField::StringIndex,
        count,
    )?;

    // Module paths share the validated string table instead of allocating duplicate text.
    for _ in 0..count {
        segments.push(Arc::clone(read_string(reader, strings, context)?));
    }

    ModulePathKey::try_new(segments)
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::Identity))
}

fn decode_declaration_identity(
    reader: &mut WireReader<'_>,
    strings: &[Arc<str>],
    context: InterfaceValidationContext,
) -> Result<ExternalDeclarationIdentity, InterfaceValidationError> {
    let shape = read_u32(reader, context, InterfaceValidationField::Discriminant)?;
    match shape {
        1 => Ok(ExternalDeclarationIdentity::Name(symbol_name(
            read_string(reader, strings, context)?,
            context,
        )?)),
        2 => Ok(ExternalDeclarationIdentity::Ordinal(SymbolOrdinal::new(
            read_u32(reader, context, InterfaceValidationField::Ordinal)?,
        ))),
        _ => Err(invalid_discriminant(
            context,
            InterfaceValidationField::Discriminant,
            shape,
        )),
    }
}

fn local_owner(
    container: Option<InterfaceSymbolId>,
    symbols: &[ImportedSymbolIdentityInput],
    context: InterfaceValidationContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    // External keys are Arc-backed immutable identities and are cheap to share while decoding.
    container
        .and_then(InterfaceSymbolId::to_index)
        .and_then(|index| symbols.get(index))
        .map(|symbol| symbol.key().clone())
        .ok_or_else(|| {
            malformed(
                context,
                InterfaceMalformedCause::InvalidReference {
                    field: InterfaceValidationField::Container,
                    index: container.map_or(u64::MAX, |id| u64::from(id.raw())),
                    available: symbols.len() as u64,
                },
            )
        })
}

const fn numeric_overflow(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    value: u64,
) -> InterfaceValidationError {
    malformed(
        context,
        InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target: InterfaceIntegerTarget::Usize,
        },
    )
}
