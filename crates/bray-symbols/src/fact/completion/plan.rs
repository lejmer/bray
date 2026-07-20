use std::collections::BTreeSet;

use bray_base::Cancellation;

use crate::{AnySymbolId, RuntimeDefaultPresence, SymbolGraph};

use super::policy::SYMBOL_FACT_KINDS;
use super::{SymbolCompletionLevel, SymbolFactKind};

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
        C: Cancellation + ?Sized,
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
        (SymbolFactKind::PredicateDefinition, AnySymbolId::Predicate(id)) => graph
            .predicate(id)
            .is_some_and(|predicate| predicate.origin() != crate::SymbolOrigin::CompilerProvided),
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
        C: Cancellation + ?Sized,
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

#[cfg(test)]
mod tests {
    use super::{SymbolCompletionPlan, SymbolCompletionPlanError, SymbolFactCompletionRequest};
    use crate::test_support::declaration_table;
    use crate::{
        AnySymbolId, NeverCancelSymbolCompletion, PackageIdentity, SymbolCompletionLevel,
        SymbolFactKind, SymbolGraph, SymbolId, SymbolKind,
    };

    #[test]
    fn recursive_completion_uses_owned_preorder_and_applicable_facts() {
        let graph = graph(&["module app; func make<T, const N: Int>(first: T = 1, second: T) {}"]);
        let package = AnySymbolId::from(graph.packages()[0].id());

        let plan = completion_plan(&graph, package);

        let symbols = plan
            .units()
            .iter()
            .map(|unit| unit.symbol().kind())
            .collect::<Vec<_>>();

        assert_eq!(
            symbols,
            [
                SymbolKind::Package,
                SymbolKind::Module,
                SymbolKind::Function,
                SymbolKind::GenericTypeParameter,
                SymbolKind::GenericConstParameter,
                SymbolKind::CallableParameter,
                SymbolKind::CallableParameterDefaultProvider,
                SymbolKind::CallableParameter,
            ]
        );

        let function = &plan.units()[2];

        assert_eq!(
            function.facts(),
            [
                SymbolFactKind::Directives,
                SymbolFactKind::GenericParameters,
                SymbolFactKind::GenericConstraints,
                SymbolFactKind::CallableSignature,
                SymbolFactKind::CallableContracts,
            ]
        );

        assert!(plan.units()[0].facts().is_empty());
        assert!(plan.units()[6].facts().is_empty());

        let default_requests = plan
            .requests()
            .iter()
            .filter(|request| request.kind() == SymbolFactKind::CallableParameterDefault)
            .collect::<Vec<_>>();

        assert_eq!(default_requests.len(), 1);
        assert_eq!(default_requests[0].symbol(), plan.units()[5].symbol());
    }

    #[test]
    fn identity_completion_traverses_without_forcing_facts() {
        let graph = graph(&["module app; func main() {}"]);
        let package = AnySymbolId::from(graph.packages()[0].id());

        let plan = match graph.completion_plan(
            package,
            SymbolCompletionLevel::Identity,
            &NeverCancelSymbolCompletion,
        ) {
            Ok(plan) => plan,
            Err(error) => panic!("identity completion should build: {error:?}"),
        };

        assert_eq!(plan.units().len(), 3);
        assert!(plan.requests().is_empty());
        assert!(plan.units().iter().all(|unit| unit.facts().is_empty()));
    }

    #[test]
    fn recovered_symbols_and_defaults_remain_completable() {
        let graph = graph(&["module app; func make(value: Int = ) {}"]);
        let package = AnySymbolId::from(graph.packages()[0].id());

        let plan = completion_plan(&graph, package);

        assert!(plan.units().iter().any(|unit| matches!(
            unit.symbol(),
            AnySymbolId::CallableParameterDefaultProvider(_)
        )));

        assert!(
            plan.requests()
                .iter()
                .any(|request| request.kind() == SymbolFactKind::CallableParameterDefault)
        );
    }

    #[test]
    fn absent_field_defaults_are_not_forced() {
        let graph = graph(&[
            "module app; struct Config { first: Int; second: Int = 1; } union Choice { Value(first: Int, second: Int = 1); }",
        ]);
        let package = AnySymbolId::from(graph.packages()[0].id());

        let plan = completion_plan(&graph, package);
        let default_requests = plan
            .requests()
            .iter()
            .filter(|request| {
                matches!(
                    request.kind(),
                    SymbolFactKind::StructFieldDefault | SymbolFactKind::UnionPayloadFieldDefault
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(default_requests.len(), 2);
    }

    #[test]
    fn referenced_semantic_recursion_does_not_expand_owned_completion() {
        let graph = graph(&["module app; func recurse() { recurse() }"]);
        let Some(function) = graph
            .functions()
            .iter()
            .find(|function| function.origin() == crate::SymbolOrigin::Source)
        else {
            panic!("test source should declare one source function");
        };

        let function = AnySymbolId::from(function.id());

        let plan = completion_plan(&graph, function);

        assert_eq!(plan.units().len(), 1);
        assert!(
            plan.requests()
                .iter()
                .all(|request| request.symbol() == function)
        );
    }

    #[test]
    fn compiler_provided_predicates_do_not_require_source_definitions() {
        let graph = graph(&["module app;"]);
        let root = AnySymbolId::from(graph.compiler_known_environment().id());
        let plan = completion_plan(&graph, root);

        let provided_predicates = plan
            .units()
            .iter()
            .filter_map(|unit| match unit.symbol() {
                AnySymbolId::Predicate(id) => graph.predicate(id),
                _ => None,
            })
            .filter(|predicate| predicate.origin() == crate::SymbolOrigin::CompilerProvided)
            .count();

        assert_eq!(provided_predicates, 3);

        assert!(plan.requests().iter().all(|request| {
            request.kind() != SymbolFactKind::PredicateDefinition
                || !matches!(request.symbol(), AnySymbolId::Predicate(_))
        }));
    }

    #[test]
    fn completion_rejects_symbols_outside_the_graph() {
        let graph = graph(&["module app; func main() {}"]);
        let unknown =
            AnySymbolId::Function(crate::FunctionSymbolId::from_symbol_id(SymbolId::new(99)));

        assert_eq!(
            graph.completion_plan(
                unknown,
                SymbolCompletionLevel::DeclarationSurface,
                &NeverCancelSymbolCompletion,
            ),
            Err(SymbolCompletionPlanError::UnknownSymbol(unknown))
        );
    }

    #[test]
    fn plan_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolCompletionPlan>();
        assert_send_sync::<SymbolFactCompletionRequest>();
    }

    fn completion_plan(graph: &SymbolGraph, root: AnySymbolId) -> SymbolCompletionPlan {
        match graph.completion_plan(
            root,
            SymbolCompletionLevel::DeclarationSurface,
            &NeverCancelSymbolCompletion,
        ) {
            Ok(plan) => plan,
            Err(error) => panic!("completion plan should build: {error:?}"),
        }
    }

    fn graph(source_texts: &[&str]) -> SymbolGraph {
        let table = declaration_table(source_texts);

        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity should be valid");
        };

        let syntax = bray_syntax::SyntaxTree::compilation_unit([]);

        match SymbolGraph::build_source(package, &table, &syntax) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph should build: {error:?}"),
        }
    }
}
