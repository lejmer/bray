use bray_symbols::{SymbolKind, SymbolRelationshipKind};
use serde::Serialize;

use crate::inspection::{InspectionSymbolIdentity, InspectionSyntaxAnchor, TreeWriter};

use super::report::{InspectionSymbol, SymbolInspectionRenderError};

#[derive(Serialize)]
pub(super) struct InspectionRelationship {
    pub(super) relationship_kind: InspectionRelationshipKind,
    pub(super) symbols: Vec<InspectionSymbol>,
    references: Vec<InspectionSymbolReference>,
}

impl InspectionRelationship {
    pub(super) fn with_symbol(
        relationship_kind: InspectionRelationshipKind,
        symbol: InspectionSymbol,
    ) -> Self {
        Self {
            relationship_kind,
            symbols: vec![symbol],
            references: Vec::new(),
        }
    }

    pub(super) fn with_references(
        relationship_kind: InspectionRelationshipKind,
        references: Vec<InspectionSymbolReference>,
    ) -> Self {
        Self {
            relationship_kind,
            symbols: Vec::new(),
            references,
        }
    }

    pub(super) fn push_text(&self, writer: &mut TreeWriter, is_last: bool) {
        writer.push_line(is_last, self.relationship_kind.group_text());
        writer.enter_children(is_last);

        let child_count = self.symbols.len() + self.references.len();
        let mut child_index = 0;

        for symbol in &self.symbols {
            child_index += 1;
            symbol.push_text(writer, child_index == child_count);
        }

        for reference in &self.references {
            child_index += 1;
            writer.push_line(child_index == child_count, &reference.text());
        }

        writer.leave_children();
    }
}

#[derive(Serialize)]
#[serde(tag = "reference_kind", rename_all = "snake_case")]
pub(super) enum InspectionSymbolReference {
    Symbol(InspectionSymbolIdentity),
    Syntax(InspectionSyntaxAnchor),
}

impl InspectionSymbolReference {
    fn text(&self) -> String {
        match self {
            Self::Symbol(symbol) => format!("symbol_reference {}", symbol.text()),
            Self::Syntax(anchor) => format!("syntax_reference {}", anchor.text()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum InspectionRelationshipKind {
    PackageModule,
    ModuleMember,
    TypeMember,
    TraitMember,
    ImplementationMember,
    StructField,
    UnionVariant,
    UnionPayloadField,
    GenericParameter,
    CallableParameter,
    PredicateParameter,
    OverloadArm,
    ImplementationFulfillment,
    DefaultProvider,
    CompilerKnownModule,
    CompilerKnownMember,
}

impl InspectionRelationshipKind {
    fn group_text(self) -> &'static str {
        match self {
            Self::PackageModule | Self::CompilerKnownModule => "modules",
            Self::ModuleMember | Self::CompilerKnownMember => "members",
            Self::TypeMember => "type_members",
            Self::TraitMember => "trait_members",
            Self::ImplementationMember => "implementation_members",
            Self::StructField => "fields",
            Self::UnionVariant => "variants",
            Self::UnionPayloadField => "payload_fields",
            Self::GenericParameter => "generic_parameters",
            Self::CallableParameter => "parameters",
            Self::PredicateParameter => "predicate_parameters",
            Self::OverloadArm => "overload_arms",
            Self::ImplementationFulfillment => "fulfillments",
            Self::DefaultProvider => "default_providers",
        }
    }
}

impl From<SymbolRelationshipKind> for InspectionRelationshipKind {
    fn from(relationship: SymbolRelationshipKind) -> Self {
        match relationship {
            SymbolRelationshipKind::PackageModule => Self::PackageModule,
            SymbolRelationshipKind::ModuleMember => Self::ModuleMember,
            SymbolRelationshipKind::TypeMember => Self::TypeMember,
            SymbolRelationshipKind::TraitMember => Self::TraitMember,
            SymbolRelationshipKind::ImplementationMember => Self::ImplementationMember,
            SymbolRelationshipKind::StructField => Self::StructField,
            SymbolRelationshipKind::UnionVariant => Self::UnionVariant,
            SymbolRelationshipKind::UnionPayloadField => Self::UnionPayloadField,
            SymbolRelationshipKind::GenericParameter => Self::GenericParameter,
            SymbolRelationshipKind::CallableParameter => Self::CallableParameter,
            SymbolRelationshipKind::PredicateParameter => Self::PredicateParameter,
            SymbolRelationshipKind::OverloadArm => Self::OverloadArm,
            SymbolRelationshipKind::ImplementationFulfillment => Self::ImplementationFulfillment,
            SymbolRelationshipKind::DefaultProvider => Self::DefaultProvider,
        }
    }
}

pub(super) fn relationship_kind(
    owner: SymbolKind,
    member: SymbolKind,
) -> Result<InspectionRelationshipKind, SymbolInspectionRenderError> {
    if let Some(relationship) = SymbolRelationshipKind::between(owner, member) {
        return Ok(relationship.into());
    }

    if owner == SymbolKind::CompilerKnownEnvironment && member == SymbolKind::Module {
        return Ok(InspectionRelationshipKind::CompilerKnownModule);
    }

    if owner == SymbolKind::CompilerKnownEnvironment {
        return Ok(InspectionRelationshipKind::CompilerKnownMember);
    }

    if owner.is_implementation() {
        return Ok(InspectionRelationshipKind::ImplementationMember);
    }

    Err(SymbolInspectionRenderError::UnsupportedRelationship)
}
