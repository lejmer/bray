use bray_symbols::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData, ModulePathKey,
    PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal, SynthesizedSymbolRole,
};

use crate::decode::{DecodeBudget, wire_error};
use crate::tag::WireTag;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause, InterfaceUtf8Failure,
    InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
};

pub(crate) fn write_external_key(encoder: &mut WireEncoder, key: &ExternalSymbolKey) {
    let mut components = Vec::new();
    let mut current = Some(key);

    while let Some(component) = current {
        components.push(component);
        current = component.owner();
    }

    encoder.write_u32(checked_u32(components.len()));

    for component in components.into_iter().rev() {
        encoder.write_u32(component.kind().to_wire());

        match component.data() {
            ExternalSymbolKeyData::Package(package) => {
                encoder.write_u32(1);
                write_string(encoder, package.as_str());
            }
            ExternalSymbolKeyData::Module { path, .. } => {
                encoder.write_u32(2);
                encoder.write_u32(checked_u32(path.segments().len()));

                for segment in path.segments() {
                    write_string(encoder, segment);
                }
            }
            ExternalSymbolKeyData::Declaration { identity, .. } => {
                encoder.write_u32(3);

                match identity {
                    ExternalDeclarationIdentity::Name(name) => {
                        encoder.write_u32(1);
                        write_string(encoder, name.as_str());
                    }
                    ExternalDeclarationIdentity::Ordinal(ordinal) => {
                        encoder.write_u32(2);
                        encoder.write_u32(ordinal.raw());
                    }
                }
            }
            ExternalSymbolKeyData::Synthesized { role, ordinal, .. } => {
                encoder.write_u32(4);
                encoder.write_u32(role.to_wire());
                write_optional_u32(encoder, ordinal.map(SymbolOrdinal::raw));
            }
        }
    }
}

pub(crate) fn read_external_key(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let root_context = InterfaceValidationContext::ExternalSymbolKey { component: 0 };
    let count = read_count(
        reader,
        budget,
        root_context,
        InterfaceValidationField::RecordCount,
        InterfaceLimit::ExternalReferenceCount,
    )?;

    if count == 0 {
        return Err(InterfaceValidationError::Malformed {
            context: root_context,
            cause: InterfaceMalformedCause::CountMismatch {
                field: InterfaceValidationField::RecordCount,
                expected: 1,
                actual: 0,
            },
        });
    }

    budget.charge_external_reference(count)?;

    let mut key = None;

    for component in 0..count {
        let context = InterfaceValidationContext::ExternalSymbolKey {
            component: component as u64,
        };
        let raw_kind = reader
            .read_u32()
            .map_err(wire_error(context, InterfaceValidationField::SymbolKind))?;
        let kind = SymbolKind::from_wire(raw_kind).ok_or(InterfaceValidationError::Malformed {
            context,
            cause: InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::SymbolKind,
                actual: u64::from(raw_kind),
            },
        })?;

        let shape = reader
            .read_u32()
            .map_err(wire_error(context, InterfaceValidationField::Discriminant))?;

        key = Some(match shape {
            1 if key.is_none() && kind == SymbolKind::Package => {
                let package = PackageIdentity::try_new(read_string(
                    reader,
                    budget.limits(),
                    context,
                    InterfaceValidationField::PackageName,
                )?)
                .ok_or(InterfaceValidationError::Malformed {
                    context,
                    cause: InterfaceMalformedCause::InvalidValue {
                        field: InterfaceValidationField::PackageName,
                    },
                })?;

                ExternalSymbolKey::package(package)
            }
            2 if kind == SymbolKind::Module => {
                let owner = key.take().ok_or(InterfaceValidationError::Malformed {
                    context,
                    cause: InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Owner,
                    },
                })?;
                let segment_count = read_count(
                    reader,
                    budget,
                    context,
                    InterfaceValidationField::RecordCount,
                    InterfaceLimit::RecordCount,
                )?;

                let mut segments = budget.allocate_items_with_minimum(
                    reader,
                    context,
                    InterfaceValidationField::RecordPayload,
                    segment_count,
                    5,
                )?;

                for _ in 0..segment_count {
                    segments.push(read_string(
                        reader,
                        budget.limits(),
                        context,
                        InterfaceValidationField::String,
                    )?);
                }

                let path = ModulePathKey::try_new(segments).ok_or(
                    InterfaceValidationError::Malformed {
                        context,
                        cause: InterfaceMalformedCause::InvalidValue {
                            field: InterfaceValidationField::Value,
                        },
                    },
                )?;

                ExternalSymbolKey::module(owner, path).ok_or(
                    InterfaceValidationError::Malformed {
                        context,
                        cause: InterfaceMalformedCause::InvalidValue {
                            field: InterfaceValidationField::Identity,
                        },
                    },
                )?
            }
            3 => {
                let owner = key.take().ok_or(InterfaceValidationError::Malformed {
                    context,
                    cause: InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Owner,
                    },
                })?;

                let identity = reader
                    .read_u32()
                    .map_err(wire_error(context, InterfaceValidationField::Identity))?;

                match identity {
                    1 => {
                        let name = SymbolName::try_new(read_string(
                            reader,
                            budget.limits(),
                            context,
                            InterfaceValidationField::SymbolName,
                        )?)
                        .ok_or(InterfaceValidationError::Malformed {
                            context,
                            cause: InterfaceMalformedCause::InvalidValue {
                                field: InterfaceValidationField::SymbolName,
                            },
                        })?;

                        ExternalSymbolKey::named(owner, kind, name)
                    }
                    2 => ExternalSymbolKey::ordinal(
                        owner,
                        kind,
                        SymbolOrdinal::new(
                            reader
                                .read_u32()
                                .map_err(wire_error(context, InterfaceValidationField::Ordinal))?,
                        ),
                    ),
                    actual => {
                        return Err(InterfaceValidationError::Malformed {
                            context,
                            cause: InterfaceMalformedCause::InvalidDiscriminant {
                                field: InterfaceValidationField::Identity,
                                actual: u64::from(actual),
                            },
                        });
                    }
                }
                .ok_or(InterfaceValidationError::Malformed {
                    context,
                    cause: InterfaceMalformedCause::InvalidValue {
                        field: InterfaceValidationField::Identity,
                    },
                })?
            }
            4 => {
                let owner = key.take().ok_or(InterfaceValidationError::Malformed {
                    context,
                    cause: InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Owner,
                    },
                })?;

                let raw_role = reader
                    .read_u32()
                    .map_err(wire_error(context, InterfaceValidationField::Role))?;
                let role = SynthesizedSymbolRole::from_wire(raw_role).ok_or(
                    InterfaceValidationError::Malformed {
                        context,
                        cause: InterfaceMalformedCause::InvalidDiscriminant {
                            field: InterfaceValidationField::Role,
                            actual: u64::from(raw_role),
                        },
                    },
                )?;

                let ordinal_discriminant = reader
                    .read_u32()
                    .map_err(wire_error(context, InterfaceValidationField::Ordinal))?;
                let ordinal = match ordinal_discriminant {
                    0 => None,
                    1 => Some(SymbolOrdinal::new(reader.read_u32().map_err(
                        wire_error(context, InterfaceValidationField::Ordinal),
                    )?)),
                    actual => {
                        return Err(InterfaceValidationError::Malformed {
                            context,
                            cause: InterfaceMalformedCause::InvalidDiscriminant {
                                field: InterfaceValidationField::Ordinal,
                                actual: u64::from(actual),
                            },
                        });
                    }
                };

                let key = ExternalSymbolKey::synthesized(owner, role, ordinal).ok_or(
                    InterfaceValidationError::Malformed {
                        context,
                        cause: InterfaceMalformedCause::InvalidValue {
                            field: InterfaceValidationField::Identity,
                        },
                    },
                )?;

                if key.kind() != kind {
                    return Err(InterfaceValidationError::Malformed {
                        context,
                        cause: InterfaceMalformedCause::CountMismatch {
                            field: InterfaceValidationField::SymbolKind,
                            expected: u64::from(key.kind().to_wire()),
                            actual: u64::from(kind.to_wire()),
                        },
                    });
                }

                key
            }
            actual => {
                return Err(InterfaceValidationError::Malformed {
                    context,
                    cause: InterfaceMalformedCause::InvalidDiscriminant {
                        field: InterfaceValidationField::Discriminant,
                        actual: u64::from(actual),
                    },
                });
            }
        });
    }

    key.ok_or(InterfaceValidationError::Malformed {
        context: root_context,
        cause: InterfaceMalformedCause::Missing {
            field: InterfaceValidationField::Identity,
        },
    })
}

fn read_count(
    reader: &mut WireReader<'_>,
    budget: &DecodeBudget,
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    limit: InterfaceLimit,
) -> Result<usize, InterfaceValidationError> {
    let count = reader.read_u32().map_err(wire_error(context, field))?;

    budget.limits().check(limit, u64::from(count))?;

    usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::NumericOverflow {
            field,
            value: u64::from(count),
            target: InterfaceIntegerTarget::Usize,
        },
    })
}

fn write_optional_u32(encoder: &mut WireEncoder, value: Option<u32>) {
    match value {
        Some(value) => {
            encoder.write_u32(1);
            encoder.write_u32(value);
        }
        None => encoder.write_u32(0),
    }
}

fn write_string(encoder: &mut WireEncoder, value: &str) {
    encoder.write_u32(checked_u32(value.len()));
    encoder.write_bytes(value.as_bytes());
}

fn read_string(
    reader: &mut WireReader<'_>,
    limits: crate::InterfaceValidationLimits,
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> Result<std::sync::Arc<str>, InterfaceValidationError> {
    let length = reader.read_u32().map_err(wire_error(context, field))?;

    limits.check(InterfaceLimit::StringLength, u64::from(length))?;

    let length = usize::try_from(length).map_err(|_| InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::NumericOverflow {
            field,
            value: u64::from(length),
            target: InterfaceIntegerTarget::Usize,
        },
    })?;
    let offset = reader.position();
    let bytes = reader
        .read_bytes(length)
        .map_err(wire_error(context, field))?;
    let value =
        std::str::from_utf8(bytes).map_err(|error| InterfaceValidationError::InvalidUtf8 {
            context,
            field,
            offset: (offset + error.valid_up_to()) as u64,
            length: length as u64,
            cause: match error.error_len() {
                Some(error_length) => InterfaceUtf8Failure::InvalidSequence {
                    error_length: Some(error_length as u64),
                },
                None => InterfaceUtf8Failure::IncompleteSequence,
            },
        })?;

    if value.is_empty() {
        return Err(InterfaceValidationError::Malformed {
            context,
            cause: InterfaceMalformedCause::InvalidValue { field },
        });
    }

    Ok(std::sync::Arc::from(value))
}

fn checked_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
