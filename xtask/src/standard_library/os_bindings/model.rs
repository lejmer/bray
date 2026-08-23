use std::collections::BTreeSet;

use bray_target::NativeTarget;
use serde::Deserialize;

pub(super) const FORMAT: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Description {
    pub(super) format: u32,
    pub(super) targets: Vec<TargetDescription>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetDescription {
    pub(super) target: String,
    pub(super) system: String,
    pub(super) sdk: SdkDescription,
    #[serde(default)]
    pub(super) links: Vec<LinkDescription>,
    #[serde(default)]
    pub(super) scalars: Vec<ScalarDescription>,
    #[serde(default)]
    pub(super) constants: Vec<ConstantDescription>,
    #[serde(default)]
    pub(super) types: Vec<TypeDescription>,
    #[serde(default)]
    pub(super) callbacks: Vec<CallbackDescription>,
    #[serde(default)]
    pub(super) functions: Vec<FunctionDescription>,
    #[serde(default)]
    pub(super) statics: Vec<StaticDescription>,
    #[serde(default)]
    pub(super) dynamic_symbols: Vec<DynamicSymbolDescription>,
    #[serde(default)]
    pub(super) overrides: Vec<OverrideDescription>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SdkDescription {
    pub(super) authority: String,
    pub(super) revision: String,
    pub(super) headers: Vec<String>,
    #[serde(default)]
    pub(super) probe_definitions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkDescription {
    pub(super) name: String,
    pub(super) kind: LinkKind,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LinkKind {
    Dynamic,
    Framework,
    Static,
    System,
}

impl LinkKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Dynamic => "dynamic",
            Self::Framework => "framework",
            Self::Static => "static",
            Self::System => "system",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScalarDescription {
    pub(super) native: String,
    pub(super) bray: String,
    pub(super) size: u64,
    pub(super) align: u64,
    pub(super) signed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConstantDescription {
    pub(super) name: String,
    pub(super) native: String,
    #[serde(rename = "type")]
    pub(super) ty: String,
    pub(super) value: i128,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TypeDescription {
    Structure(AggregateDescription),
    Union(AggregateDescription),
    Incomplete(IncompleteDescription),
    Opaque(OpaqueDescription),
    Flexible(FlexibleDescription),
    Bitfields(BitfieldDescription),
}

impl TypeDescription {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Structure(description) | Self::Union(description) => &description.name,
            Self::Incomplete(description) => &description.name,
            Self::Opaque(description) => &description.name,
            Self::Flexible(description) => &description.name,
            Self::Bitfields(description) => &description.name,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AggregateDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) size: u64,
    pub(super) align: u64,
    pub(super) classification: AggregateClassification,
    pub(super) fields: Vec<FieldDescription>,
    #[serde(default)]
    pub(super) active_variant_contract: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AggregateClassification {
    Aggregate,
    Integer,
    Memory,
}

impl AggregateClassification {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Aggregate => "aggregate",
            Self::Integer => "integer",
            Self::Memory => "memory",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FieldDescription {
    pub(super) name: String,
    pub(super) native: String,
    #[serde(rename = "type")]
    pub(super) ty: String,
    pub(super) native_type: String,
    pub(super) offset: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncompleteDescription {
    pub(super) name: String,
    pub(super) native: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OpaqueDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) size: u64,
    pub(super) align: u64,
    pub(super) classification: AggregateClassification,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FlexibleDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) align: u64,
    pub(super) classification: AggregateClassification,
    pub(super) fields: Vec<FieldDescription>,
    pub(super) tail: FlexibleTailDescription,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FlexibleTailDescription {
    pub(super) name: String,
    pub(super) native: String,
    #[serde(rename = "type")]
    pub(super) ty: String,
    pub(super) native_type: String,
    pub(super) offset: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BitfieldDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) size: u64,
    pub(super) align: u64,
    pub(super) classification: AggregateClassification,
    pub(super) backing_name: String,
    pub(super) backing_type: String,
    pub(super) native_backing_type: String,
    pub(super) backing_offset: u64,
    pub(super) bitfields: Vec<BitfieldMemberDescription>,
    #[serde(default)]
    pub(super) fields: Vec<FieldDescription>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BitfieldMemberDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) shift: u8,
    pub(super) width: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CallbackDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) abi: CallableAbi,
    pub(super) parameters: Vec<ParameterDescription>,
    pub(super) result: String,
    pub(super) native_result: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionDescription {
    pub(super) name: String,
    pub(super) native: String,
    pub(super) abi: CallableAbi,
    pub(super) parameters: Vec<ParameterDescription>,
    pub(super) result: String,
    pub(super) native_result: String,
    #[serde(default)]
    pub(super) variadic: bool,
    pub(super) link: String,
    #[serde(default)]
    pub(super) symbol: Option<SymbolDescription>,
    #[serde(default)]
    pub(super) error_state: Option<String>,
    #[serde(default)]
    pub(super) ownership: Option<String>,
    #[serde(default)]
    pub(super) initialization: Option<String>,
    #[serde(default)]
    pub(super) dependencies: Vec<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CallableAbi {
    C,
    System,
}

impl CallableAbi {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::System => "system",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ParameterDescription {
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) ty: String,
    pub(super) native_type: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaticDescription {
    pub(super) name: String,
    pub(super) native: String,
    #[serde(rename = "type")]
    pub(super) ty: String,
    pub(super) native_type: String,
    pub(super) link: String,
    #[serde(default)]
    pub(super) mutable: bool,
    #[serde(default)]
    pub(super) thread_local: bool,
    pub(super) symbol: SymbolDescription,
    #[serde(default)]
    pub(super) ownership: Option<String>,
    #[serde(default)]
    pub(super) dependencies: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SymbolDescription {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) ordinal: Option<u32>,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default = "default_binding")]
    pub(super) binding: SymbolBinding,
    #[serde(default = "default_presence")]
    pub(super) presence: SymbolPresence,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SymbolBinding {
    Strong,
    Weak,
}

impl SymbolBinding {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Weak => "weak",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SymbolPresence {
    Optional,
    Required,
}

impl SymbolPresence {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }
}

const fn default_binding() -> SymbolBinding {
    SymbolBinding::Strong
}

const fn default_presence() -> SymbolPresence {
    SymbolPresence::Required
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DynamicSymbolDescription {
    pub(super) name: String,
    pub(super) library: String,
    pub(super) symbol: String,
    pub(super) target: DynamicSymbolTarget,
    #[serde(rename = "type")]
    pub(super) ty: String,
    #[serde(default)]
    pub(super) ownership: Option<String>,
    #[serde(default)]
    pub(super) dependencies: Vec<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DynamicSymbolTarget {
    Code,
    Data,
}

impl DynamicSymbolTarget {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Data => "data",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OverrideDescription {
    pub(super) fact: String,
    pub(super) value: String,
    pub(super) authority: String,
    pub(super) reason: String,
}

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
    validate_text("SDK revision", &target.sdk.revision)?;

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
        target.constants.iter().map(|constant| constant.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "type",
        target.types.iter().map(TypeDescription::name),
    )?;

    validate_unique_names(
        &target.target,
        "callback",
        target.callbacks.iter().map(|callback| callback.name.as_str()),
    )?;

    validate_unique_names(
        &target.target,
        "function",
        target.functions.iter().map(|function| function.name.as_str()),
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

    validate_unique_names(
        &target.target,
        "override",
        target.overrides.iter().map(|override_| override_.fact.as_str()),
    )?;

    validate_links(target)?;
    validate_types(target)?;
    validate_symbols(target, native_target)?;

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

                validate_distinct_names(
                    &target.target,
                    "field",
                    aggregate.fields.iter().map(|field| field.name.as_str()),
                )?;
            }
            TypeDescription::Opaque(opaque) => {
                validate_layout(target, &opaque.name, opaque.size, opaque.align)?;
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

                let bit_width = match bitfields.backing_type.as_str() {
                    "u8" => 8,
                    "u16" => 16,
                    "u32" => 32,
                    "u64" => 64,
                    _ => 0,
                };

                for field in &bitfields.bitfields {
                    if field.width == 0 || u16::from(field.shift) + u16::from(field.width) > bit_width {
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

fn validate_layout(target: &TargetDescription, name: &str, size: u64, align: u64) -> Result<(), String> {
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

fn validate_symbols(
    target: &TargetDescription,
    native_target: NativeTarget,
) -> Result<(), String> {
    for function in &target.functions {
        if let Some(symbol) = &function.symbol {
            validate_symbol(target, native_target, &function.name, symbol, false)?;
        }
    }

    for static_ in &target.statics {
        validate_symbol(
            target,
            native_target,
            &static_.name,
            &static_.symbol,
            true,
        )?;
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

#[cfg(test)]
mod tests {
    use super::{Description, validate};

    #[test]
    fn checked_in_description_covers_every_native_target() {
        let description: Description = serde_json::from_str(include_str!(
            "../../../../standard-library/targets/os-bindings.json"
        ))
        .unwrap_or_else(|error| panic!("binding description must parse: {error}"));

        validate(&description)
            .unwrap_or_else(|error| panic!("binding description must be valid: {error}"));
    }
}
