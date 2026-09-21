use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, NativeSymbolContract,
    NativeSymbolIdentity, NativeSymbolPresence,
};
use bray_target::{NativeTarget, ObjectFormat};
use serde::{Deserialize, Serialize};

use crate::index::{NativeArtifactIndex, NativeIndexError};
use crate::model::{
    NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest, NativeDefinition,
    NativeDefinitionSelection, NativeRoot, NativeUnit, NativeUnitKind, NativeUnitSummary,
};

const FORMAT: &str = "bray_native_units";
const REVISION: u16 = 1;

/// Invalid or unsupported native index wire field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireError {
    /// Schema marker or revision is incompatible.
    UnsupportedSchema,
    /// Target or object format is invalid.
    InvalidTarget,
    /// Content identity is not a lowercase 32-byte digest.
    InvalidDigest,
    /// Native symbol identity or policy is malformed.
    InvalidSymbol,
    /// Link requirement is malformed.
    InvalidLink,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndexWire {
    format: String,
    revision: u16,
    target: String,
    object_format: String,
    producer: String,
    units: Vec<UnitWire>,
    co_retention_groups: Vec<Vec<String>>,
}

impl IndexWire {
    pub(super) fn from_index(index: &NativeArtifactIndex) -> Self {
        Self {
            format: FORMAT.to_owned(),
            revision: REVISION,
            target: index.target().as_str().to_owned(),
            object_format: object_format_name(index.object_format()).to_owned(),
            producer: index.producer().to_string(),
            units: index.units().iter().map(UnitWire::from_unit).collect(),
            co_retention_groups: index
                .co_retention_groups()
                .iter()
                .map(|group| group.members().iter().map(ToString::to_string).collect())
                .collect(),
        }
    }

    pub(super) fn into_index(self) -> Result<NativeArtifactIndex, NativeIndexError> {
        if self.format != FORMAT || self.revision != REVISION {
            return Err(NativeIndexError::Wire(WireError::UnsupportedSchema));
        }

        let target = NativeTarget::ALL
            .into_iter()
            .find(|target| target.as_str() == self.target)
            .ok_or(NativeIndexError::Wire(WireError::InvalidTarget))?;

        if self.object_format != object_format_name(target.object_format()) {
            return Err(NativeIndexError::Wire(WireError::InvalidTarget));
        }

        let producer = digest(&self.producer)?;

        let units = self
            .units
            .into_iter()
            .map(UnitWire::into_unit)
            .collect::<Result<Vec<_>, _>>()?;

        let groups = self
            .co_retention_groups
            .into_iter()
            .map(|members| {
                let members = members
                    .iter()
                    .map(|member| digest(member))
                    .collect::<Result<Vec<_>, _>>()?;

                NativeCoRetentionGroup::try_new(members)
                    .ok_or(NativeIndexError::InvalidCoRetentionGroup)
            })
            .collect::<Result<Vec<_>, _>>()?;

        NativeArtifactIndex::try_new(target, producer, units, groups)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnitWire {
    digest: String,
    kind: UnitKindWire,
    summary: SummaryWire,
    native_links: Vec<LinkWire>,
    link_options: Vec<String>,
}

impl UnitWire {
    fn from_unit(unit: &NativeUnit) -> Self {
        Self {
            digest: unit.digest().to_string(),
            kind: UnitKindWire::from_kind(unit.kind()),
            summary: SummaryWire::from_summary(unit.summary()),
            native_links: unit
                .native_links()
                .iter()
                .map(LinkWire::from_link)
                .collect(),
            link_options: unit
                .link_options()
                .iter()
                .map(|item| item.to_string())
                .collect(),
        }
    }

    fn into_unit(self) -> Result<NativeUnit, NativeIndexError> {
        let digest = digest(&self.digest)?;

        let links = self
            .native_links
            .into_iter()
            .map(LinkWire::into_link)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(NativeUnit::new(
            digest,
            self.kind.into_kind(),
            self.summary.into_summary()?,
            links,
            self.link_options.into_iter().map(Arc::<str>::from),
        ))
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum UnitKindWire {
    Object,
    Bitcode,
    OpaqueArchive,
}

impl UnitKindWire {
    const fn from_kind(kind: NativeUnitKind) -> Self {
        match kind {
            NativeUnitKind::Object => Self::Object,
            NativeUnitKind::Bitcode => Self::Bitcode,
            NativeUnitKind::OpaqueArchive => Self::OpaqueArchive,
        }
    }

    const fn into_kind(self) -> NativeUnitKind {
        match self {
            Self::Object => NativeUnitKind::Object,
            Self::Bitcode => NativeUnitKind::Bitcode,
            Self::OpaqueArchive => NativeUnitKind::OpaqueArchive,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SummaryWire {
    Exact {
        definitions: Vec<DefinitionWire>,
        references: Vec<SymbolWire>,
        roots: Vec<RootWire>,
    },
    Opaque,
}

impl SummaryWire {
    fn from_summary(summary: &NativeUnitSummary) -> Self {
        match summary {
            NativeUnitSummary::Exact {
                definitions,
                references,
                roots,
            } => Self::Exact {
                definitions: definitions
                    .iter()
                    .map(DefinitionWire::from_definition)
                    .collect(),
                references: references.iter().map(SymbolWire::from_symbol).collect(),
                roots: roots.iter().copied().map(RootWire::from_root).collect(),
            },
            NativeUnitSummary::Opaque => Self::Opaque,
        }
    }

    fn into_summary(self) -> Result<NativeUnitSummary, NativeIndexError> {
        match self {
            Self::Exact {
                definitions,
                references,
                roots,
            } => Ok(NativeUnitSummary::Exact {
                definitions: definitions
                    .into_iter()
                    .map(DefinitionWire::into_definition)
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
                references: references
                    .into_iter()
                    .map(SymbolWire::into_symbol)
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
                roots: roots
                    .into_iter()
                    .map(RootWire::into_root)
                    .collect::<Vec<_>>()
                    .into(),
            }),
            Self::Opaque => Ok(NativeUnitSummary::Opaque),
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum RootWire {
    Initialization,
    Finalization,
}

impl RootWire {
    const fn from_root(root: NativeRoot) -> Self {
        match root {
            NativeRoot::Initialization => Self::Initialization,
            NativeRoot::Finalization => Self::Finalization,
        }
    }

    const fn into_root(self) -> NativeRoot {
        match self {
            Self::Initialization => NativeRoot::Initialization,
            Self::Finalization => NativeRoot::Finalization,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DefinitionWire {
    symbol: SymbolWire,
    selection: SelectionWire,
}

impl DefinitionWire {
    fn from_definition(definition: &NativeDefinition) -> Self {
        Self {
            symbol: SymbolWire::from_symbol(definition.symbol()),
            selection: SelectionWire::from_selection(definition.selection()),
        }
    }

    fn into_definition(self) -> Result<NativeDefinition, NativeIndexError> {
        Ok(NativeDefinition::new(
            self.symbol.into_symbol()?,
            self.selection.into_selection()?,
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionWire {
    Ordinary,
    Fallback,
    Comdat {
        group: IdentityWire,
        rule: ComdatRuleWire,
        associative_with: Option<IdentityWire>,
    },
}

impl SelectionWire {
    fn from_selection(selection: &NativeDefinitionSelection) -> Self {
        match selection {
            NativeDefinitionSelection::Ordinary => Self::Ordinary,
            NativeDefinitionSelection::Fallback => Self::Fallback,
            NativeDefinitionSelection::Comdat {
                group,
                rule,
                associative_with,
            } => Self::Comdat {
                group: IdentityWire::from_identity(group),
                rule: ComdatRuleWire::from_rule(*rule),
                associative_with: associative_with.as_ref().map(IdentityWire::from_identity),
            },
        }
    }

    fn into_selection(self) -> Result<NativeDefinitionSelection, NativeIndexError> {
        match self {
            Self::Ordinary => Ok(NativeDefinitionSelection::Ordinary),
            Self::Fallback => Ok(NativeDefinitionSelection::Fallback),
            Self::Comdat {
                group,
                rule,
                associative_with,
            } => Ok(NativeDefinitionSelection::Comdat {
                group: group.into_identity()?,
                rule: rule.into_rule(),
                associative_with: associative_with
                    .map(IdentityWire::into_identity)
                    .transpose()?,
            }),
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ComdatRuleWire {
    Any,
    SameSize,
    ExactMatch,
    Largest,
    NoDuplicates,
}

impl ComdatRuleWire {
    const fn from_rule(rule: NativeComdatSelection) -> Self {
        match rule {
            NativeComdatSelection::Any => Self::Any,
            NativeComdatSelection::SameSize => Self::SameSize,
            NativeComdatSelection::ExactMatch => Self::ExactMatch,
            NativeComdatSelection::Largest => Self::Largest,
            NativeComdatSelection::NoDuplicates => Self::NoDuplicates,
        }
    }

    const fn into_rule(self) -> NativeComdatSelection {
        match self {
            Self::Any => NativeComdatSelection::Any,
            Self::SameSize => NativeComdatSelection::SameSize,
            Self::ExactMatch => NativeComdatSelection::ExactMatch,
            Self::Largest => NativeComdatSelection::Largest,
            Self::NoDuplicates => NativeComdatSelection::NoDuplicates,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SymbolWire {
    identity: IdentityWire,
    version: Option<String>,
    binding: BindingWire,
    presence: PresenceWire,
}

impl SymbolWire {
    fn from_symbol(symbol: &NativeSymbolContract) -> Self {
        Self {
            identity: IdentityWire::from_identity(symbol.identity()),
            version: symbol.version().map(str::to_owned),
            binding: BindingWire::from_binding(symbol.binding()),
            presence: PresenceWire::from_presence(symbol.presence()),
        }
    }

    fn into_symbol(self) -> Result<NativeSymbolContract, NativeIndexError> {
        let version = self
            .version
            .map(|value| {
                NonEmptySharedStr::try_new(value)
                    .ok_or(NativeIndexError::Wire(WireError::InvalidSymbol))
            })
            .transpose()?;

        Ok(NativeSymbolContract::new(
            self.identity.into_identity()?,
            version,
            self.binding.into_binding(),
            self.presence.into_presence(),
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum IdentityWire {
    Name(String),
    Ordinal(u64),
}

impl IdentityWire {
    fn from_identity(identity: &NativeSymbolIdentity) -> Self {
        match identity {
            NativeSymbolIdentity::Name(name) => Self::Name(name.as_str().to_owned()),
            NativeSymbolIdentity::Ordinal(ordinal) => Self::Ordinal(*ordinal),
        }
    }

    fn into_identity(self) -> Result<NativeSymbolIdentity, NativeIndexError> {
        match self {
            Self::Name(name) => NonEmptySharedStr::try_new(name)
                .map(NativeSymbolIdentity::Name)
                .ok_or(NativeIndexError::Wire(WireError::InvalidSymbol)),
            Self::Ordinal(ordinal) => Ok(NativeSymbolIdentity::Ordinal(ordinal)),
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BindingWire {
    Strong,
    Weak,
}

impl BindingWire {
    const fn from_binding(binding: NativeSymbolBinding) -> Self {
        match binding {
            NativeSymbolBinding::Strong => Self::Strong,
            NativeSymbolBinding::Weak => Self::Weak,
        }
    }

    const fn into_binding(self) -> NativeSymbolBinding {
        match self {
            Self::Strong => NativeSymbolBinding::Strong,
            Self::Weak => NativeSymbolBinding::Weak,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum PresenceWire {
    Required,
    Optional,
}

impl PresenceWire {
    const fn from_presence(presence: NativeSymbolPresence) -> Self {
        match presence {
            NativeSymbolPresence::Required => Self::Required,
            NativeSymbolPresence::Optional => Self::Optional,
        }
    }

    const fn into_presence(self) -> NativeSymbolPresence {
        match self {
            Self::Required => NativeSymbolPresence::Required,
            Self::Optional => NativeSymbolPresence::Optional,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LinkWire {
    name: String,
    kind: String,
}

impl LinkWire {
    fn from_link(link: &NativeLinkRequirement) -> Self {
        Self {
            name: link.name().to_owned(),
            kind: link.kind().as_str().to_owned(),
        }
    }

    fn into_link(self) -> Result<NativeLinkRequirement, NativeIndexError> {
        let name = NonEmptySharedStr::try_new(self.name)
            .ok_or(NativeIndexError::Wire(WireError::InvalidLink))?;

        let kind = NativeLinkKind::for_name(&self.kind)
            .ok_or(NativeIndexError::Wire(WireError::InvalidLink))?;

        Ok(NativeLinkRequirement::new(name, kind))
    }
}

fn digest(value: &str) -> Result<NativeContentDigest, NativeIndexError> {
    NativeContentDigest::from_hex(value).ok_or(NativeIndexError::Wire(WireError::InvalidDigest))
}

const fn object_format_name(format: ObjectFormat) -> &'static str {
    match format {
        ObjectFormat::Coff => "coff",
        ObjectFormat::Elf => "elf",
        ObjectFormat::MachO => "mach_o",
        ObjectFormat::WebAssembly => "web_assembly",
        ObjectFormat::Xcoff => "xcoff",
    }
}
