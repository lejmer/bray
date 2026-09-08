//! Typed storage-plan inspection records.

use bray_bound_tree::{
    BorrowCapabilityOrigin, PlannedBorrowCapability, StorageAccess, StorageAccessPlan,
    StorageAccessRoot, StorageAlternative, StorageBinding, StorageBindingTarget, StorageIdentity,
    StoragePlan, StorageProjection,
};
use bray_symbols::{SemanticValueStore, SymbolGraph};
use serde::Serialize;

use crate::inspection::{
    InspectionSourceError, InspectionSources, InspectionSymbolIdentity, InspectionSyntaxAnchor,
    InspectionType, TypeInspectionError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StorageInspectionError {
    Capacity {
        resource: &'static str,
        actual: usize,
    },
    Source(InspectionSourceError),
    Type(TypeInspectionError),
}

impl From<InspectionSourceError> for StorageInspectionError {
    fn from(error: InspectionSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<TypeInspectionError> for StorageInspectionError {
    fn from(error: TypeInspectionError) -> Self {
        Self::Type(error)
    }
}

#[derive(Serialize)]
pub(super) struct InspectionStorage {
    identities: Vec<InspectionStorageIdentity>,
    accesses: Vec<InspectionStorageAccess>,
    alternatives: Vec<InspectionStorageAlternative>,
    borrow_capabilities: Vec<InspectionBorrowCapability>,
    bindings: Vec<InspectionStorageBindingEntry>,
    plans: Vec<InspectionStoragePlan>,
    owned_borrows: Vec<InspectionOwnedBorrow>,
}

#[derive(Serialize)]
struct InspectionOwnedBorrow {
    owner: InspectionType,
    kind: &'static str,
    callable: InspectionSymbolIdentity,
    signature: InspectionType,
    parameter: InspectionType,
    result: InspectionType,
}

impl InspectionStorage {
    pub(super) fn from_plan(
        plan: &StoragePlan,
        symbols: &SymbolGraph,
        semantic_values: &SemanticValueStore,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, StorageInspectionError> {
        let identities = plan
            .identity_entries()
            .map(|(id, identity)| {
                InspectionStorageIdentity::new(id.ordinal(), identity, symbols, sources)
            })
            .collect::<Result<_, _>>()?;

        let accesses = plan
            .access_entries()
            .map(|(id, access)| {
                InspectionStorageAccess::new(
                    id.ordinal(),
                    access,
                    symbols,
                    semantic_values,
                    sources,
                )
            })
            .collect::<Result<_, _>>()?;

        let alternatives = plan
            .alternatives()
            .iter()
            .enumerate()
            .map(|(id, alternative)| InspectionStorageAlternative::new(id, alternative))
            .collect::<Result<_, _>>()?;

        let borrow_capabilities = plan
            .borrow_capability_entries()
            .map(|(id, capability)| {
                InspectionBorrowCapability::new(id.ordinal(), capability, sources)
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            identities,
            accesses,
            alternatives,
            borrow_capabilities,
            owned_borrows: plan
                .owned_borrows()
                .map(|(owner, kind, call)| {
                    Ok(InspectionOwnedBorrow {
                        owner: InspectionType::from_type(semantic_values, symbols, owner)?,
                        kind: kind.as_str(),
                        callable: InspectionSymbolIdentity::from_symbol(
                            symbols,
                            call.callable().definition().symbol(),
                        ),
                        signature: InspectionType::from_type(
                            semantic_values,
                            symbols,
                            call.callable_type(),
                        )?,
                        parameter: InspectionType::from_type(
                            semantic_values,
                            symbols,
                            call.parameter(),
                        )?,
                        result: InspectionType::from_type(semantic_values, symbols, call.result())?,
                    })
                })
                .collect::<Result<_, StorageInspectionError>>()?,
            bindings: plan
                .bindings()
                .iter()
                .copied()
                .map(InspectionStorageBindingEntry::from)
                .collect(),
            plans: plan
                .access_plans()
                .iter()
                .copied()
                .map(InspectionStoragePlan::from)
                .collect(),
        })
    }

    pub(super) fn identity_count(&self) -> usize {
        self.identities.len()
    }

    pub(super) fn access_count(&self) -> usize {
        self.accesses.len()
    }

    pub(super) fn plans(&self) -> &[InspectionStoragePlan] {
        &self.plans
    }

    pub(super) fn accesses(&self) -> &[InspectionStorageAccess] {
        &self.accesses
    }
}

#[derive(Serialize)]
struct InspectionStorageIdentity {
    id: u32,
    storage_kind: &'static str,
    provenance: InspectionStorageProvenance,
}

impl InspectionStorageIdentity {
    fn new(
        id: u32,
        identity: StorageIdentity,
        symbols: &SymbolGraph,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, StorageInspectionError> {
        Ok(Self {
            id,
            storage_kind: identity.kind_name(),
            provenance: InspectionStorageProvenance::new(identity, symbols, sources)?,
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "provenance_kind", rename_all = "snake_case")]
enum InspectionStorageProvenance {
    Node {
        node_kind: &'static str,
        node: u32,
    },
    Expression {
        expression: u32,
    },
    SurfaceSymbol {
        symbol: InspectionSymbolIdentity,
    },
    LocalSymbol {
        symbol_kind: &'static str,
        id: u32,
    },
    CompilerCreated {
        source: InspectionSyntaxAnchor,
        synthesis_role: Option<&'static str>,
        synthesis_ordinal: Option<u32>,
    },
    Alternative {
        pattern: u32,
        alternative: u32,
    },
    Error {
        source: InspectionSyntaxAnchor,
    },
}

impl InspectionStorageProvenance {
    fn new(
        identity: StorageIdentity,
        symbols: &SymbolGraph,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, StorageInspectionError> {
        match identity {
            StorageIdentity::LocalOwned(node) | StorageIdentity::Result(node) => Ok(Self::Node {
                node_kind: node.kind().as_str(),
                node: node.ordinal(),
            }),
            StorageIdentity::Temporary(expression)
            | StorageIdentity::CustomIndexBorrow(expression)
            | StorageIdentity::IterationCursor(expression)
            | StorageIdentity::IterationElement(expression)
            | StorageIdentity::Allocation(expression) => Ok(Self::Expression {
                expression: expression.ordinal(),
            }),
            StorageIdentity::Parameter(symbol) => Ok(Self::SurfaceSymbol {
                symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol.into()),
            }),
            StorageIdentity::Receiver(symbol) => Ok(Self::SurfaceSymbol {
                symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol.into()),
            }),
            StorageIdentity::Static(symbol) => Ok(Self::SurfaceSymbol {
                symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol.into()),
            }),
            StorageIdentity::PredicateParameter(symbol) => Ok(Self::SurfaceSymbol {
                symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol.into()),
            }),
            StorageIdentity::AnonymousParameter(symbol) => Ok(Self::LocalSymbol {
                symbol_kind: "anonymous_callable_parameter",
                id: symbol.ordinal(),
            }),
            StorageIdentity::ContractParameter(symbol) => Ok(Self::LocalSymbol {
                symbol_kind: "local_binding",
                id: symbol.ordinal(),
            }),
            StorageIdentity::PostconditionResult(symbol) => Ok(Self::LocalSymbol {
                symbol_kind: "postcondition_result",
                id: symbol.ordinal(),
            }),
            StorageIdentity::CompilerCreated(origin) => {
                let synthesis = origin.synthesized_origin();

                Ok(Self::CompilerCreated {
                    source: InspectionSyntaxAnchor::from_anchor(
                        sources,
                        origin.source_anchor().syntax(),
                    )?,
                    synthesis_role: synthesis.map(|origin| origin.role().as_str()),
                    synthesis_ordinal: synthesis.map(|origin| origin.ordinal().raw()),
                })
            }
            StorageIdentity::Alternative {
                pattern,
                alternative,
            } => Ok(Self::Alternative {
                pattern: pattern.ordinal(),
                alternative: alternative.ordinal(),
            }),
            StorageIdentity::Error(source) => Ok(Self::Error {
                source: InspectionSyntaxAnchor::from_anchor(sources, source.syntax())?,
            }),
        }
    }
}

#[derive(Serialize)]
pub(super) struct InspectionStorageAccess {
    id: u32,
    root: InspectionStorageAccessRoot,
    projections: Vec<InspectionStorageProjection>,
    reached_type: InspectionType,
    source: InspectionSyntaxAnchor,
    recovered: bool,
}

impl InspectionStorageAccess {
    fn new(
        id: u32,
        access: &StorageAccess,
        symbols: &SymbolGraph,
        semantic_values: &SemanticValueStore,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, StorageInspectionError> {
        Ok(Self {
            id,
            root: access.root().into(),
            projections: access
                .projections()
                .iter()
                .copied()
                .map(|projection| InspectionStorageProjection::new(projection, symbols))
                .collect(),
            reached_type: InspectionType::from_type(
                semantic_values,
                symbols,
                access.reached_type(),
            )?,
            source: InspectionSyntaxAnchor::from_anchor(sources, access.source().syntax())?,
            recovered: access.is_recovered(),
        })
    }

    pub(super) fn text(&self) -> String {
        let projections = if self.projections.is_empty() {
            String::new()
        } else {
            format!(
                " projections={}",
                self.projections
                    .iter()
                    .map(InspectionStorageProjection::text)
                    .collect::<Vec<_>>()
                    .join(".")
            )
        };

        let recovered = if self.recovered { " [recovered]" } else { "" };

        format!(
            "access:{} {}{projections} type={} {}{recovered}",
            self.id,
            self.root.text(),
            self.reached_type.text(),
            self.source.location_text()
        )
    }
}

#[derive(Serialize)]
#[serde(tag = "root_kind", rename_all = "snake_case")]
enum InspectionStorageAccessRoot {
    Storage {
        identity: u32,
    },
    Borrow {
        capability: u32,
    },
    BorrowedStorage {
        capability: u32,
        identity: u32,
    },
    OwnedIndirection {
        owner_expression: u32,
        identity: u32,
    },
    Recovery {
        identity: u32,
    },
}

impl InspectionStorageAccessRoot {
    fn text(&self) -> String {
        match self {
            Self::Storage { identity } => format!("storage:{identity}"),
            Self::Borrow { capability } => format!("borrow:{capability}"),
            Self::BorrowedStorage {
                capability,
                identity,
            } => format!("borrow:{capability} storage:{identity}"),
            Self::OwnedIndirection {
                owner_expression,
                identity,
            } => format!("owned expression:{owner_expression} storage:{identity}"),
            Self::Recovery { identity } => format!("recovery storage:{identity}"),
        }
    }
}

impl From<StorageAccessRoot> for InspectionStorageAccessRoot {
    fn from(root: StorageAccessRoot) -> Self {
        match root {
            StorageAccessRoot::Storage(identity) => Self::Storage {
                identity: identity.ordinal(),
            },
            StorageAccessRoot::Borrow(capability) => Self::Borrow {
                capability: capability.ordinal(),
            },
            StorageAccessRoot::BorrowedStorage {
                capability,
                storage,
            } => Self::BorrowedStorage {
                capability: capability.ordinal(),
                identity: storage.ordinal(),
            },
            StorageAccessRoot::OwnedIndirection { owner, storage } => Self::OwnedIndirection {
                owner_expression: owner.ordinal(),
                identity: storage.ordinal(),
            },
            StorageAccessRoot::Recovery(identity) => Self::Recovery {
                identity: identity.ordinal(),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "projection_kind", rename_all = "snake_case")]
enum InspectionStorageProjection {
    ProductField {
        field: InspectionSymbolIdentity,
    },
    TupleElement {
        ordinal: u32,
    },
    ElementFromStart {
        ordinal: u32,
    },
    ElementFromEnd {
        ordinal: u32,
    },
    ActiveUnionPayloadField {
        variant: InspectionSymbolIdentity,
        field: InspectionSymbolIdentity,
    },
    Element {
        selector_expression: u32,
    },
    SliceRange {
        start_expression: Option<u32>,
        end_expression: Option<u32>,
    },
    NullableValue,
    OwnedTarget,
}

impl InspectionStorageProjection {
    fn new(projection: StorageProjection, symbols: &SymbolGraph) -> Self {
        match projection {
            StorageProjection::ProductField(field) => Self::ProductField {
                field: InspectionSymbolIdentity::from_symbol(symbols, field.into()),
            },
            StorageProjection::TupleElement(ordinal) => Self::TupleElement {
                ordinal: ordinal.raw(),
            },
            StorageProjection::ElementFromStart(ordinal) => Self::ElementFromStart {
                ordinal: ordinal.raw(),
            },
            StorageProjection::ElementFromEnd(ordinal) => Self::ElementFromEnd {
                ordinal: ordinal.raw(),
            },
            StorageProjection::ActiveUnionPayloadField { variant, field } => {
                Self::ActiveUnionPayloadField {
                    variant: InspectionSymbolIdentity::from_symbol(symbols, variant.into()),
                    field: InspectionSymbolIdentity::from_symbol(symbols, field.into()),
                }
            }
            StorageProjection::Element(selector) => Self::Element {
                selector_expression: selector.ordinal(),
            },
            StorageProjection::SliceRange { start, end } => Self::SliceRange {
                start_expression: start.map(|expression| expression.ordinal()),
                end_expression: end.map(|expression| expression.ordinal()),
            },
            StorageProjection::NullableValue => Self::NullableValue,
            StorageProjection::OwnedTarget => Self::OwnedTarget,
        }
    }

    fn text(&self) -> String {
        match self {
            Self::ProductField { field } => field.display_name().into_owned(),
            Self::TupleElement { ordinal } => format!("tuple:{ordinal}"),
            Self::ElementFromStart { ordinal } => format!("from_start:{ordinal}"),
            Self::ElementFromEnd { ordinal } => format!("from_end:{ordinal}"),
            Self::ActiveUnionPayloadField { variant, field } => {
                format!("{}:{}", variant.display_name(), field.display_name())
            }
            Self::Element {
                selector_expression,
            } => format!("element:{selector_expression}"),
            Self::SliceRange {
                start_expression,
                end_expression,
            } => format!(
                "slice:{}:{}",
                optional_ordinal(*start_expression),
                optional_ordinal(*end_expression)
            ),
            Self::NullableValue => String::from("nullable_value"),
            Self::OwnedTarget => String::from("owned_target"),
        }
    }
}

fn optional_ordinal(ordinal: Option<u32>) -> String {
    ordinal.map_or_else(|| String::from("_"), |ordinal| ordinal.to_string())
}

#[derive(Serialize)]
struct InspectionStorageAlternative {
    id: u32,
    pattern: u32,
    accesses: Vec<u32>,
}

impl InspectionStorageAlternative {
    fn new(id: usize, alternative: &StorageAlternative) -> Result<Self, StorageInspectionError> {
        let id = u32::try_from(id).map_err(|_| StorageInspectionError::Capacity {
            resource: "inspection_storage_alternative_count",
            actual: id,
        })?;

        Ok(Self {
            id,
            pattern: alternative.pattern().ordinal(),
            accesses: alternative
                .accesses()
                .iter()
                .map(|access| access.ordinal())
                .collect(),
        })
    }
}

#[derive(Serialize)]
struct InspectionBorrowCapability {
    id: u32,
    origin: InspectionBorrowCapabilityOrigin,
    borrow_kind: &'static str,
    access: u32,
    parent: Option<u32>,
    source: InspectionSyntaxAnchor,
    recovered: bool,
}

impl InspectionBorrowCapability {
    fn new(
        id: u32,
        capability: PlannedBorrowCapability,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, StorageInspectionError> {
        Ok(Self {
            id,
            origin: capability.origin().into(),
            borrow_kind: capability.kind().as_str(),
            access: capability.access().ordinal(),
            parent: capability.parent().map(|parent| parent.ordinal()),
            source: InspectionSyntaxAnchor::from_anchor(sources, capability.source().syntax())?,
            recovered: capability.is_recovered(),
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "origin_kind", rename_all = "snake_case")]
enum InspectionBorrowCapabilityOrigin {
    Expression {
        expression: u32,
    },
    Entry {
        target: InspectionStorageBindingTarget,
    },
}

impl From<BorrowCapabilityOrigin> for InspectionBorrowCapabilityOrigin {
    fn from(origin: BorrowCapabilityOrigin) -> Self {
        match origin {
            BorrowCapabilityOrigin::Expression(expression) => Self::Expression {
                expression: expression.ordinal(),
            },
            BorrowCapabilityOrigin::Entry(target) => Self::Entry {
                target: target.into(),
            },
        }
    }
}

#[derive(Serialize)]
struct InspectionStorageBindingEntry {
    target: InspectionStorageBindingTarget,
    binding: InspectionStorageBinding,
}

impl From<(StorageBindingTarget, StorageBinding)> for InspectionStorageBindingEntry {
    fn from((target, binding): (StorageBindingTarget, StorageBinding)) -> Self {
        Self {
            target: target.into(),
            binding: binding.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
enum InspectionStorageBindingTarget {
    SurfaceSymbol { symbol_kind: &'static str, id: u32 },
    LocalSymbol { symbol_kind: &'static str, id: u32 },
    Result,
}

impl From<StorageBindingTarget> for InspectionStorageBindingTarget {
    fn from(target: StorageBindingTarget) -> Self {
        match target {
            StorageBindingTarget::Parameter(symbol) => Self::SurfaceSymbol {
                symbol_kind: "callable_parameter",
                id: symbol.symbol_id().raw(),
            },
            StorageBindingTarget::Receiver(symbol) => Self::SurfaceSymbol {
                symbol_kind: "receiver_parameter",
                id: symbol.symbol_id().raw(),
            },
            StorageBindingTarget::Static(symbol) => Self::SurfaceSymbol {
                symbol_kind: "static",
                id: symbol.symbol_id().raw(),
            },
            StorageBindingTarget::PredicateParameter(symbol) => Self::SurfaceSymbol {
                symbol_kind: "predicate_parameter",
                id: symbol.symbol_id().raw(),
            },
            StorageBindingTarget::AnonymousParameter(symbol) => Self::LocalSymbol {
                symbol_kind: "anonymous_callable_parameter",
                id: symbol.ordinal(),
            },
            StorageBindingTarget::Local(symbol) => Self::LocalSymbol {
                symbol_kind: "local_binding",
                id: symbol.ordinal(),
            },
            StorageBindingTarget::PatternDiscard(pattern) => Self::LocalSymbol {
                symbol_kind: "pattern_discard",
                id: pattern.ordinal(),
            },
            StorageBindingTarget::PatternSubject(pattern) => Self::LocalSymbol {
                symbol_kind: "pattern_subject",
                id: pattern.ordinal(),
            },
            StorageBindingTarget::PostconditionResult(symbol) => Self::LocalSymbol {
                symbol_kind: "postcondition_result",
                id: symbol.ordinal(),
            },
            StorageBindingTarget::Result => Self::Result,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "binding_kind", rename_all = "snake_case")]
enum InspectionStorageBinding {
    Identity { identity: u32 },
    Access { access: u32 },
}

impl From<StorageBinding> for InspectionStorageBinding {
    fn from(binding: StorageBinding) -> Self {
        match binding {
            StorageBinding::Identity(identity) => Self::Identity {
                identity: identity.ordinal(),
            },
            StorageBinding::Access(access) => Self::Access {
                access: access.ordinal(),
            },
        }
    }
}

#[derive(Clone, Serialize)]
pub(super) struct InspectionStoragePlan {
    node_kind: &'static str,
    node: u32,
    expression: u32,
    purpose: &'static str,
    access: u32,
}

impl InspectionStoragePlan {
    pub(super) const fn node_kind(&self) -> &'static str {
        self.node_kind
    }

    pub(super) const fn node(&self) -> u32 {
        self.node
    }

    pub(super) const fn expression(&self) -> u32 {
        self.expression
    }

    pub(super) const fn purpose(&self) -> &'static str {
        self.purpose
    }

    pub(super) const fn access(&self) -> u32 {
        self.access
    }
}

impl From<StorageAccessPlan> for InspectionStoragePlan {
    fn from(plan: StorageAccessPlan) -> Self {
        Self {
            node_kind: plan.node().kind().as_str(),
            node: plan.node().ordinal(),
            expression: plan.expression().ordinal(),
            purpose: plan.purpose().as_str(),
            access: plan.access().ordinal(),
        }
    }
}
