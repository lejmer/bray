use std::collections::BTreeSet;

use bray_target::NativeTarget;

use super::model::{
    Description, FORMAT, OverrideAuthority, ParameterDescription, SdkRevision, SymbolBinding,
    SymbolDescription, SymbolPresence, TargetDescription, TypeDescription, is_primitive_abi_type,
};

pub(super) fn validate(description: &Description) -> Result<(), String> {
    if description.format != FORMAT {
        return Err(format!(
            "unsupported OS binding description format {}",
            description.format
        ));
    }

    let expected = NativeTarget::ALL.map(NativeTarget::as_str);

    let actual = description
        .targets
        .iter()
        .map(|target| target.target.as_str())
        .collect::<Vec<_>>();

    if actual != expected {
        return Err(format!(
            "target descriptions must be ordered exactly as {}",
            expected.join(", ")
        ));
    }

    for target in &description.targets {
        validate_target(target)?;
    }

    Ok(())
}
fn validate_target(target: &TargetDescription) -> Result<(), String> {
    validate_text("SDK authority", &target.sdk.authority)?;
    validate_sdk_revision(target)?;

    if target.sdk.headers.is_empty() {
        return Err(format!("{} defines no SDK headers", target.target));
    }

    let native_target = NativeTarget::ALL
        .into_iter()
        .find(|native| native.as_str() == target.target)
        .ok_or_else(|| format!("unsupported native target {}", target.target))?;

    let expected_system = native_target
        .profile()
        .properties()
        .identity()
        .system()
        .to_owned();

    if target.system != expected_system {
        return Err(format!(
            "{} must use system {}",
            target.target, expected_system
        ));
    }

    validate_unique_names(
        &target.target,
        "header",
        target.sdk.headers.iter().map(String::as_str),
    )?;

    validate_unique_names(
        &target.target,
        "link",
        target.links.iter().map(|link| link.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "scalar",
        target.scalars.iter().map(|scalar| scalar.native.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "constant",
        target
            .constants
            .iter()
            .map(|constant| constant.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "type",
        target.types.iter().map(TypeDescription::name),
    )?;

    validate_unique_names(
        &target.target,
        "callback",
        target
            .callbacks
            .iter()
            .map(|callback| callback.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "function",
        target
            .functions
            .iter()
            .map(|function| function.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "static",
        target.statics.iter().map(|static_| static_.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "dynamic symbol",
        target
            .dynamic_symbols
            .iter()
            .map(|symbol| symbol.name.as_str()),
    )?;

    validate_links(target)?;
    validate_types(target)?;
    validate_abi_types(target)?;
    validate_symbols(target, native_target)?;

    Ok(())
}

fn validate_sdk_revision(target: &TargetDescription) -> Result<(), String> {
    let revision_system = match &target.sdk.revision {
        SdkRevision::Linux {
            distribution,
            release,
            kernel,
            glibc,
        } => {
            validate_text("Linux SDK distribution", distribution)?;
            validate_text("Linux SDK distribution release", release)?;
            validate_text("Linux SDK kernel revision", kernel)?;
            validate_text("Linux SDK glibc revision", glibc)?;
            validate_revision_pair("Linux SDK kernel revision", kernel)?;
            validate_revision_pair("Linux SDK glibc revision", glibc)?;

            "linux"
        }
        SdkRevision::MacOs { version } => {
            validate_text("macOS SDK revision", version)?;
            validate_revision_pair("macOS SDK revision", version)?;

            "darwin"
        }
        SdkRevision::Windows { version } => {
            validate_text("Windows SDK revision", version)?;

            "windows"
        }
    };

    if revision_system != target.system {
        return Err(format!(
            "{} uses an SDK revision for {revision_system}",
            target.target
        ));
    }

    match (&target.sdk.revision, &target.sdk.compiler_runtime) {
        (SdkRevision::Windows { .. }, Some(runtime)) => {
            validate_text("compiler runtime authority", &runtime.authority)?;
            validate_text("compiler runtime revision", &runtime.revision)?;
        }
        (SdkRevision::Windows { .. }, None) => {
            return Err(format!(
                "{} must identify its Windows compiler runtime",
                target.target
            ));
        }
        (_, Some(_)) => {
            return Err(format!(
                "{} has an unexpected compiler runtime",
                target.target
            ));
        }
        (_, None) => {}
    }

    Ok(())
}

fn validate_revision_pair(field: &str, revision: &str) -> Result<(), String> {
    let mut parts = revision.split('.');

    if parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .is_none()
        || parts
            .next()
            .and_then(|part| part.parse::<u32>().ok())
            .is_none()
        || parts.next().is_some()
    {
        return Err(format!(
            "{field} must contain numeric major and minor components"
        ));
    }

    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), String> {
    if value.trim() != value
        || value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_graphic() || character == ' ')
    {
        return Err(format!("{field} must be nonempty single-line ASCII"));
    }

    Ok(())
}

fn validate_unique_names<'a>(
    target: &str,
    category: &str,
    names: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let mut previous = None;

    for name in names {
        if previous.is_some_and(|previous: &str| previous >= name) {
            return Err(format!(
                "{target} {category} entries must be uniquely name-sorted"
            ));
        }

        previous = Some(name);
    }

    Ok(())
}

fn validate_links(target: &TargetDescription) -> Result<(), String> {
    let links = target
        .links
        .iter()
        .map(|link| link.name.as_str())
        .collect::<BTreeSet<_>>();

    for function in &target.functions {
        if !links.contains(function.link.as_str()) {
            return Err(format!(
                "{} function {} names unknown link {}",
                target.target, function.name, function.link
            ));
        }
    }

    for static_ in &target.statics {
        if !links.contains(static_.link.as_str()) {
            return Err(format!(
                "{} static {} names unknown link {}",
                target.target, static_.name, static_.link
            ));
        }
    }

    Ok(())
}

fn validate_types(target: &TargetDescription) -> Result<(), String> {
    for ty in &target.types {
        match ty {
            TypeDescription::Structure(aggregate) | TypeDescription::Union(aggregate) => {
                validate_layout(target, &aggregate.name, aggregate.size, aggregate.align)?;
                validate_override(&aggregate.override_)?;

                validate_distinct_names(
                    &target.target,
                    "field",
                    aggregate.fields.iter().map(|field| field.name.as_str()),
                )?;
            }
            TypeDescription::Opaque(opaque) => {
                validate_layout(target, &opaque.name, opaque.size, opaque.align)?;
                validate_override(&opaque.override_)?;
            }
            TypeDescription::Flexible(flexible) => {
                if flexible.align == 0 || flexible.tail.offset == 0 {
                    return Err(format!(
                        "{} flexible type {} has an invalid layout",
                        target.target, flexible.name
                    ));
                }
            }
            TypeDescription::Bitfields(bitfields) => {
                validate_layout(target, &bitfields.name, bitfields.size, bitfields.align)?;
                validate_override(&bitfields.override_)?;

                let bit_width = match bitfields.backing_type.as_str() {
                    "u8" => 8,
                    "u16" => 16,
                    "u32" => 32,
                    "u64" => 64,
                    _ => 0,
                };

                for field in &bitfields.bitfields {
                    if field.width == 0
                        || u16::from(field.shift) + u16::from(field.width) > bit_width
                    {
                        return Err(format!(
                            "{} bitfield {}.{} exceeds its backing storage",
                            target.target, bitfields.name, field.name
                        ));
                    }
                }
            }
            TypeDescription::Incomplete(_) => {}
        }
    }

    Ok(())
}

fn validate_override(override_: &Option<OverrideAuthority>) -> Result<(), String> {
    let Some(override_) = override_ else {
        return Ok(());
    };

    validate_text("override authority", &override_.authority)?;

    validate_text("override reason", &override_.reason)
}

fn validate_abi_types(target: &TargetDescription) -> Result<(), String> {
    for ty in &target.types {
        match ty {
            TypeDescription::Structure(aggregate) | TypeDescription::Union(aggregate) => {
                for field in &aggregate.fields {
                    validate_abi_type(target, &field.ty)?;
                }
            }
            TypeDescription::Flexible(flexible) => {
                for field in &flexible.fields {
                    validate_abi_type(target, &field.ty)?;
                }

                validate_abi_type(target, &flexible.tail.ty)?;
            }
            TypeDescription::Bitfields(bitfields) => {
                validate_abi_type(target, &bitfields.backing_type)?;

                for field in &bitfields.fields {
                    validate_abi_type(target, &field.ty)?;
                }
            }
            TypeDescription::Incomplete(_) | TypeDescription::Opaque(_) => {}
        }
    }

    for callback in &target.callbacks {
        validate_callable_types(target, &callback.parameters, &callback.result)?;
    }

    for function in &target.functions {
        validate_callable_types(target, &function.parameters, &function.result)?;

        for condition in &function.requires {
            validate_text("function precondition", condition)?;
        }

        for condition in &function.ensures {
            validate_text("function postcondition", condition)?;
        }
    }

    for static_ in &target.statics {
        validate_abi_type(target, &static_.ty)?;
    }

    for symbol in &target.dynamic_symbols {
        validate_abi_type(target, &symbol.ty)?;
    }

    Ok(())
}

fn validate_callable_types(
    target: &TargetDescription,
    parameters: &[ParameterDescription],
    result: &str,
) -> Result<(), String> {
    for parameter in parameters {
        validate_abi_type(target, &parameter.ty)?;
    }

    if result == "unit" {
        Ok(())
    } else {
        validate_abi_type(target, result)
    }
}

fn validate_abi_type(target: &TargetDescription, ty: &str) -> Result<(), String> {
    if is_primitive_abi_type(ty) {
        return Ok(());
    }

    if let Some(pointee) = ty
        .strip_prefix("RawPointer<")
        .and_then(|pointee| pointee.strip_suffix('>'))
    {
        return validate_abi_type(target, pointee);
    }

    if target.types.iter().any(|candidate| candidate.name() == ty)
        || target
            .callbacks
            .iter()
            .any(|candidate| candidate.name == ty)
    {
        return Ok(());
    }

    Err(format!(
        "{} ABI type {ty} has no target representation",
        target.target
    ))
}

fn validate_layout(
    target: &TargetDescription,
    name: &str,
    size: u64,
    align: u64,
) -> Result<(), String> {
    if size == 0 || align == 0 || !align.is_power_of_two() {
        return Err(format!(
            "{} type {} has an invalid size or alignment",
            target.target, name
        ));
    }

    Ok(())
}

fn validate_distinct_names<'a>(
    target: &str,
    category: &str,
    names: impl Iterator<Item = &'a str>,
) -> Result<(), String> {
    let mut observed = BTreeSet::new();

    for name in names {
        if !observed.insert(name) {
            return Err(format!("{target} {category} entries must be unique"));
        }
    }

    Ok(())
}

fn validate_symbols(target: &TargetDescription, native_target: NativeTarget) -> Result<(), String> {
    for function in &target.functions {
        if let Some(symbol) = &function.symbol {
            validate_symbol(target, native_target, &function.name, symbol, false)?;
        }
    }

    for static_ in &target.statics {
        validate_symbol(target, native_target, &static_.name, &static_.symbol, true)?;
    }

    Ok(())
}

fn validate_symbol(
    target: &TargetDescription,
    native_target: NativeTarget,
    declaration: &str,
    symbol: &SymbolDescription,
    data: bool,
) -> Result<(), String> {
    if symbol.name.is_some() == symbol.ordinal.is_some() {
        return Err(format!(
            "{} declaration {} must select one symbol name or ordinal",
            target.target, declaration
        ));
    }

    if !data && matches!(symbol.presence, SymbolPresence::Optional) {
        return Err(format!(
            "{} callable {} cannot use optional static presence",
            target.target, declaration
        ));
    }

    let support = native_target.profile().properties().native_symbols();

    if symbol.ordinal.is_some() && !support.ordinals() {
        return Err(format!(
            "{} declaration {} uses an unsupported symbol ordinal",
            target.target, declaration
        ));
    }

    if symbol.version.is_some() && !support.versions() {
        return Err(format!(
            "{} declaration {} uses an unsupported symbol version",
            target.target, declaration
        ));
    }

    if matches!(symbol.binding, SymbolBinding::Weak) && !support.weak_binding() {
        return Err(format!(
            "{} declaration {} uses unsupported weak binding",
            target.target, declaration
        ));
    }

    if matches!(symbol.presence, SymbolPresence::Optional) && !support.optional_data() {
        return Err(format!(
            "{} declaration {} uses unsupported optional data",
            target.target, declaration
        ));
    }

    Ok(())
}
