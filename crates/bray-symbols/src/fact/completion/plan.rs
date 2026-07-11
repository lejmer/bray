use std::collections::BTreeSet;

use crate::{AnySymbolId, RuntimeDefaultPresence, SymbolGraph};

use super::policy::SYMBOL_FACT_KINDS;
use super::{SymbolCompletionCancellation, SymbolCompletionLevel, SymbolFactKind};

/// One erased symbol-fact request in a force-completion plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolFactCompletionRequest {
    symbol: AnySymbolId,
    kind: SymbolFactKind,
}

impl SymbolFactCompletionRequest {
    /// Creates an exact completion request.
    pub const fn new(symbol: AnySymbolId, kind: SymbolFactKind) -> Self {
        Self { symbol, kind }
    }

    /// Returns the symbol that owns the fact.
    pub const fn symbol(self) -> AnySymbolId {
        self.symbol
    }

    /// Returns the requested fact category.
    pub const fn kind(self) -> SymbolFactKind {
        self.kind
    }
}

/// The applicable fact requests for one symbol in canonical fact order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolCompletionUnit {
    symbol: AnySymbolId,
    facts: Box<[SymbolFactKind]>,
}

impl SymbolCompletionUnit {
    /// Returns the completed symbol identity.
    pub const fn symbol(&self) -> AnySymbolId {
        self.symbol
    }

    /// Returns applicable facts in canonical category order.
    pub fn facts(&self) -> &[SymbolFactKind] {
        &self.facts
    }
}

/// A deterministic, recursively expanded symbol-completion plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolCompletionPlan {
    level: SymbolCompletionLevel,
    units: Box<[SymbolCompletionUnit]>,
    requests: Box<[SymbolFactCompletionRequest]>,
}

impl SymbolCompletionPlan {
    pub(crate) fn build<C>(
        graph: &SymbolGraph,
        root: AnySymbolId,
        level: SymbolCompletionLevel,
        cancellation: &C,
    ) -> Result<Self, SymbolCompletionPlanError>
    where
        C: SymbolCompletionCancellation + ?Sized,
    {
        if !graph.contains_symbol(root) {
            return Err(SymbolCompletionPlanError::UnknownSymbol(root));
        }

        let mut visited = BTreeSet::new();
        let mut pending = vec![root];
        let mut units = Vec::new();
        let mut requests = Vec::new();

        while let Some(symbol) = pending.pop() {
            if cancellation.is_cancelled() {
                return Err(SymbolCompletionPlanError::Cancelled);
            }

            if !visited.insert(symbol) {
                continue;
            }

            let facts = SYMBOL_FACT_KINDS
                .into_iter()
                .filter(|kind| {
                    kind.is_required_for(level)
                        && completion_fact_is_applicable(graph, symbol, *kind)
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();

            requests.extend(
                facts
                    .iter()
                    .copied()
                    .map(|kind| SymbolFactCompletionRequest::new(symbol, kind)),
            );

            units.push(SymbolCompletionUnit { symbol, facts });

            pending.extend(graph.completion_children(symbol).iter().rev().copied());
        }

        Ok(Self {
            level,
            units: units.into_boxed_slice(),
            requests: requests.into_boxed_slice(),
        })
    }

    /// Returns the completion boundary represented by this plan.
    pub const fn level(&self) -> SymbolCompletionLevel {
        self.level
    }

    /// Returns recursively owned symbols in deterministic preorder.
    pub fn units(&self) -> &[SymbolCompletionUnit] {
        &self.units
    }

    /// Returns every applicable fact request in deterministic completion order.
    pub fn requests(&self) -> &[SymbolFactCompletionRequest] {
        &self.requests
    }
}

fn completion_fact_is_applicable(
    graph: &SymbolGraph,
    symbol: AnySymbolId,
    kind: SymbolFactKind,
) -> bool {
    if !kind.is_applicable_to(symbol) {
        return false;
    }

    match (kind, symbol) {
        (SymbolFactKind::CallableParameterDefault, AnySymbolId::CallableParameter(id)) => {
            graph.callable_parameter(id).is_some_and(|parameter| {
                parameter.default_presence() != RuntimeDefaultPresence::Absent
            })
        }
        (SymbolFactKind::StructFieldDefault, AnySymbolId::StructField(id)) => graph
            .struct_field(id)
            .is_some_and(|field| field.default_presence() != RuntimeDefaultPresence::Absent),
        (SymbolFactKind::UnionPayloadFieldDefault, AnySymbolId::UnionPayloadField(id)) => graph
            .union_payload_field(id)
            .is_some_and(|field| field.default_presence() != RuntimeDefaultPresence::Absent),
        _ => true,
    }
}

impl SymbolGraph {
    /// Builds the deterministic recursive plan for one symbol completion boundary.
    ///
    /// Referenced symbols are deliberately excluded, so recursive semantic references do not
    /// become completion cycles.
    pub fn completion_plan<C>(
        &self,
        root: AnySymbolId,
        level: SymbolCompletionLevel,
        cancellation: &C,
    ) -> Result<SymbolCompletionPlan, SymbolCompletionPlanError>
    where
        C: SymbolCompletionCancellation + ?Sized,
    {
        SymbolCompletionPlan::build(self, root, level, cancellation)
    }
}

/// Failure to construct a recursive completion plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolCompletionPlanError {
    /// Completion was cancelled before the plan was complete.
    Cancelled,
    /// The requested symbol does not belong to the graph.
    UnknownSymbol(AnySymbolId),
}
