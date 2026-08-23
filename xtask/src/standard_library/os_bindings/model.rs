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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SdkDescription {
    pub(super) authority: String,
    pub(super) revision: SdkRevision,
    pub(super) headers: Vec<String>,
    #[serde(default)]
    pub(super) probe_definitions: Vec<String>,
    #[serde(default)]
    pub(super) compiler_runtime: Option<CompilerRuntimeDescription>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompilerRuntimeDescription {
    pub(super) authority: String,
    pub(super) revision: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum SdkRevision {
    Linux {
        distribution: String,
        release: String,
        kernel: String,
        glibc: String,
    },
    MacOs {
        version: String,
    },
    Windows {
        version: String,
    },
}

impl SdkRevision {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Linux {
                distribution,
                release,
                kernel,
                glibc,
            } => format!("{distribution} {release}, Linux {kernel}, glibc {glibc}"),
            Self::MacOs { version } => version.clone(),
            Self::Windows { version } => version.clone(),
        }
    }
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
    #[serde(default, rename = "override")]
    pub(super) override_: Option<OverrideAuthority>,
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
    #[serde(default, rename = "override")]
    pub(super) override_: Option<OverrideAuthority>,
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
    #[serde(default, rename = "override")]
    pub(super) override_: Option<OverrideAuthority>,
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
pub(super) struct OverrideAuthority {
    pub(super) authority: String,
    pub(super) reason: String,
}
