// rust-style: allow(module-too-large, reason = "semantic diagnostic aggregation and its source index form one cached query boundary")

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_bound_tree::{
    BoundExpression, BoundUnit, BoundUnitKey, BoundUnitKind, CheckedAsync,
    CheckedBodyBehavior, CheckedControlFlow, CheckedDependencyContracts,
    CheckedMemoryOperations, CheckedPatterns, CheckedRefinements,
    DeclaredValueTypeTemplates, Liveness, SemanticSelection, StorageFlow, StoragePlan,
};
use bray_checker::{
    TargetCallableAbiRequirement, TargetValidityRequest, TargetValidityRequirement,
};
use bray_declarations::{DeclarationKind, DeclarationRecord, SyntaxAnchor};
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceSymbolIdentity, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticProductKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableContractsQuery, CallableSymbolId, ConstantDefinitionState,
    DeclaredTypeRepresentation, ImplementationSymbolId, ImportedSymbolSkeleton, ModuleSurface,
    ModuleSurfaceQuery, NamedTypeSymbolId, ProductKind, SemanticFactResult, SymbolFactRequest,
    SymbolFactResult, SymbolGraph, SymbolKey, SymbolOrigin, TraitImplementationConformanceQuery,
    diagnostic_symbol_identity, diagnostic_symbol_kind,
};
use bray_syntax::{
    ExpressionSyntax, SyntaxKind, SyntaxTree, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree,
};

use super::binder::has_visible_generic_parameters;
use super::constant::{constant_definition_id, empty_concrete_substitution};
use super::state::{CheckedExpressionSemantics, Compilation};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

pub(super) fn source_diagnostic(anchor: SyntaxAnchor, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(SourceSpan::new(anchor.source_id(), anchor.full_range()))
}

pub(super) fn labeled_source_diagnostic(
    anchor: SyntaxAnchor,
    kind: DiagnosticKind,
    label: DiagnosticLabelKind,
) -> Diagnostic {
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    source_diagnostic(anchor, kind).with_label(DiagnosticLabel::primary(label, span))
}

pub(super) const fn diagnostic_product_kind(kind: ProductKind) -> DiagnosticProductKind {
    match kind {
        ProductKind::Executable => DiagnosticProductKind::Executable,
        ProductKind::Library => DiagnosticProductKind::Library,
        ProductKind::Test => DiagnosticProductKind::Test,
    }
}

pub(super) fn symbol_diagnostic_identity(
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    symbol: AnySymbolId,
) -> Result<DiagnosticInterfaceSymbolIdentity, FactQueryError> {
    let key = symbols
        .symbol_key(symbol)
        .or_else(|| imported.and_then(|imported| imported.symbol_key(symbol)))
        .ok_or(FactQueryError::InfrastructureFailure)?;

    if let Some(name) = symbols.member_name(symbol)
        && let Some(owner) = symbols.containing_symbol(symbol)
        && let Some(owner_key) = symbols
            .symbol_key(owner)
            .or_else(|| imported.and_then(|imported| imported.symbol_key(owner)))
    {
        return Ok(DiagnosticInterfaceSymbolIdentity::Declaration {
            owner: Box::new(diagnostic_symbol_identity(owner_key)),
            kind: diagnostic_symbol_kind(key.data().kind()),
            identity: DiagnosticInterfaceDeclarationIdentity::Name(name.as_str().to_owned()),
        });
    }

    Ok(diagnostic_symbol_identity(key))
}

impl Compilation {
    /// Returns diagnostics produced by binding and semantic analysis of this package.
    pub fn semantic_diagnostics(&self) -> &DiagnosticBag {
        match self.semantic_diagnostics_with_cancellation(&self.state.cancellation) {
            Ok(diagnostics) => diagnostics,
            Err(FactQueryError::Cancelled) => {
                panic!("uncancellable semantic diagnostics were unexpectedly cancelled")
            }
            Err(FactQueryError::Cycle(cycle)) => {
                panic!("semantic diagnostic dependencies formed a cycle: {cycle:?}")
            }
            Err(FactQueryError::InfrastructureFailure) => {
                panic!("semantic diagnostic infrastructure failed")
            }
            Err(FactQueryError::SemanticUnitContext(error)) => {
                panic!("semantic unit context failed: {error:?}")
            }
            Err(FactQueryError::CheckerInfrastructure(error)) => {
                panic!("semantic checker infrastructure failed: {error:?}")
            }
        }
    }

    pub(super) fn semantic_diagnostics_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::SemanticDiagnostics,
            &self.state.semantic_diagnostics,
            cancellation,
            |cancellation| self.compute_semantic_diagnostics(cancellation),
        )
    }

    /// Returns diagnostics for the current whole-package check request.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        match self.check_diagnostics_with_cancellation(&self.state.cancellation) {
            Ok(diagnostics) => diagnostics,
            Err(FactQueryError::Cancelled) => {
                panic!("uncancellable check diagnostics were unexpectedly cancelled")
            }
            Err(FactQueryError::Cycle(cycle)) => {
                panic!("check diagnostic dependencies formed a cycle: {cycle:?}")
            }
            Err(FactQueryError::InfrastructureFailure) => {
                panic!("check diagnostic infrastructure failed")
            }
            Err(FactQueryError::SemanticUnitContext(error)) => {
                panic!("check diagnostic semantic unit context failed: {error:?}")
            }
            Err(FactQueryError::CheckerInfrastructure(error)) => {
                panic!("check diagnostic checker infrastructure failed: {error:?}")
            }
        }
    }

    pub(super) fn check_diagnostics_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::CheckDiagnostics,
            &self.state.check_diagnostics,
            cancellation,
            |cancellation| self.compute_check_diagnostics(cancellation),
        )
    }

    fn compute_check_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let diagnostics = self.state.fact_runtime.map_indexed(4, |index| {
            cancellation.check()?;

            let diagnostics = match index {
                0 => self.source_diagnostics(),
                1 => self.syntax_tree_result().diagnostics(),
                2 => self.imported_diagnostics(),
                3 => self.semantic_diagnostics_with_cancellation(cancellation)?,
                _ => unreachable!("scheduled diagnostic index must be in range"),
            };

            cancellation.check()?;

            Ok(diagnostics)
        })?;

        let diagnostics = diagnostics.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(DiagnosticBag::merged_all(diagnostics))
    }

    fn compute_semantic_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let source_graph = self.product_source_graph()?;
        let mut pending = BTreeSet::new();

        for key in self.declared_unit_keys()? {
            pending.insert(unit_order_key(key));
        }

        let symbols = self.symbol_graph()?;
        let mut sources = Vec::new();
        let binder = self.binding_context(cancellation)?;

        for module in symbols
            .modules()
            .iter()
            .filter(|module| module.origin() == SymbolOrigin::Source)
        {
            let surface = binder
                .symbol_fact(SymbolFactRequest::<ModuleSurfaceQuery>::new(module.id()))
                .map_err(super::binder::binder_fact_error)?;

            sources.push(SemanticDiagnosticSource::ModuleSurface(surface));
        }

        let callables = source_graph
            .declarations()
            .declarations()
            .iter()
            .filter_map(|declaration| symbols.symbol_for_declaration(declaration.id()))
            .filter_map(CallableSymbolId::try_from_any)
            .collect::<Vec<_>>();

        let contract_results = self
            .state
            .fact_runtime
            .map_indexed(callables.len(), |index| {
                cancellation.check()?;

                let callable = callables[index];

                binder
                    .symbol_fact(SymbolFactRequest::<CallableContractsQuery>::new(callable))
                    .map_err(super::binder::binder_fact_error)
            })?;

        for contracts in contract_results {
            sources.push(SemanticDiagnosticSource::CallableContracts(contracts?));
        }

        for subject in symbols
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| NamedTypeSymbolId::from(symbol.id()))
            .chain(
                symbols
                    .unions()
                    .iter()
                    .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                    .map(|symbol| NamedTypeSymbolId::from(symbol.id())),
            )
        {
            sources.push(SemanticDiagnosticSource::TypeRepresentation(
                self.declared_type_representation(subject)?,
            ));
        }

        for implementation in symbols
            .unnamed_trait_implementations()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| ImplementationSymbolId::from(symbol.id()))
            .chain(
                symbols
                    .named_trait_implementations()
                    .iter()
                    .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                    .map(|symbol| ImplementationSymbolId::from(symbol.id())),
            )
        {
            sources.push(SemanticDiagnosticSource::TraitConformance(
                self.trait_implementation_conformance(implementation)?,
            ));
        }

        while let Some((_, _, _, key)) = pending.pop_first() {
            let (bound, unit_facts) = self.semantic_unit_diagnostic_sources(key, cancellation)?;

            for nested in bound.result().value().nested_units() {
                // Nested unit keys are Arc-backed immutable identities shared with their owner.
                pending.insert(unit_order_key(nested.clone()));
            }

            sources.extend(unit_facts);
        }

        let query_diagnostics =
            DiagnosticBag::merged_all(sources.iter().map(SemanticDiagnosticSource::diagnostics));

        let coherence = self.implementation_coherence_diagnostics(cancellation)?;
        let callable_overloads = self.callable_overload_diagnostics(cancellation)?;
        let foreign_callables = self.foreign_callable_diagnostics(cancellation)?;
        let product = self.product_semantics_with_cancellation(cancellation)?;

        Ok(DiagnosticBag::merged_all([
            source_graph.diagnostics(),
            &query_diagnostics,
            coherence,
            callable_overloads,
            foreign_callables,
            product.diagnostics(),
        ]))
    }

    pub(super) fn semantic_unit_diagnostics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let (_, sources) = self.semantic_unit_diagnostic_sources(key, cancellation)?;

        Ok(DiagnosticBag::merged_all(
            sources.iter().map(SemanticDiagnosticSource::diagnostics),
        ))
    }

    fn semantic_unit_diagnostic_sources(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Arc<PublishedUnitFact<BoundUnit>>,
            Vec<SemanticDiagnosticSource>,
        ),
        FactQueryError,
    > {
        // Each source request owns the same Arc-backed unit identity independently.
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let declared_types =
            self.declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

        let embedded_constants = self.checked_constant_terms_for_templates_with_cancellation(
            declared_types
                .result()
                .value()
                .evidence()
                .iter()
                .map(|evidence| evidence.template())
                .chain(declared_types.result().value().callable_type())
                .chain(declared_types.result().value().callable_result()),
            cancellation,
        )?;

        let expression_semantics =
            self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let control_flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;
        let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;
        let refinements = self.refinements_with_cancellation(key.clone(), cancellation)?;
        let storage_flow = self.storage_flow_with_cancellation(key.clone(), cancellation)?;

        let dependencies =
            self.dependency_contracts_with_cancellation(key.clone(), cancellation)?;

        let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
        let async_analysis = self.async_analysis_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

        let target_validity = self.semantic_unit_target_validity(
            bound.result().value(),
            expression_semantics.result().value(),
            cancellation,
        )?;

        let mut sources = vec![
            SemanticDiagnosticSource::Bound(Arc::clone(bound.result())),
            SemanticDiagnosticSource::DeclaredTypes(Arc::clone(declared_types.result())),
            SemanticDiagnosticSource::EmbeddedConstants(embedded_constants),
            SemanticDiagnosticSource::ExpressionSemantics(expression_semantics),
            SemanticDiagnosticSource::ControlFlow(Arc::clone(control_flow.result())),
            SemanticDiagnosticSource::Patterns(Arc::clone(patterns.result())),
            SemanticDiagnosticSource::Storage(Arc::clone(storage.result())),
            SemanticDiagnosticSource::Liveness(Arc::clone(liveness.result())),
            SemanticDiagnosticSource::Refinements(Arc::clone(refinements.result())),
            SemanticDiagnosticSource::StorageFlow(Arc::clone(storage_flow.result())),
            SemanticDiagnosticSource::Dependencies(Arc::clone(dependencies.result())),
            SemanticDiagnosticSource::Memory(Arc::clone(memory.result())),
            SemanticDiagnosticSource::Async(Arc::clone(async_analysis.result())),
            SemanticDiagnosticSource::BodyBehavior(Arc::clone(behavior.result())),
            SemanticDiagnosticSource::TargetValidity(target_validity),
        ];

        if key.kind() == BoundUnitKind::ConstantTemplate {
            let symbols = self.symbol_graph()?;

            let owner = symbols
                .symbol_for_key(key.declared_owner())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let definition =
                constant_definition_id(owner).ok_or(FactQueryError::InfrastructureFailure)?;

            let template = self.constant_definition(definition)?;

            sources.push(SemanticDiagnosticSource::ConstantTemplate(template));

            if !has_visible_generic_parameters(symbols, owner) {
                let substitution =
                    empty_concrete_substitution(self.semantic_value_store()?, definition)?;

                let instance =
                    bray_symbols::ConstantInstanceKey::new(definition, substitution, None);

                let value = self.constant_instance_with_cancellation(instance, cancellation)?;

                sources.push(SemanticDiagnosticSource::ConstantInstance(value));
            }
        }

        Ok((bound, sources))
    }

    pub(in crate::compilation) fn declared_unit_keys(
        &self,
    ) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let syntax = self.syntax_tree();
        let declarations = self.product_source_graph()?.declarations();
        let syntax_index = SemanticSyntaxIndex::new(syntax, declarations.declarations());

        let mut keys = Vec::new();

        for declaration in declarations.declarations() {
            let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            // Stable symbol keys are Arc-backed and retained by each bound-unit key.
            let owner = symbols
                .symbol_key(symbol)
                .cloned()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            self.push_primary_unit_key(&mut keys, declaration, owner.clone(), &syntax_index)?;

            self.push_surface_unit_keys(
                &mut keys,
                declaration,
                symbol,
                owner,
                symbols,
                &syntax_index,
            )?;
        }

        Ok(keys)
    }

    #[cfg(test)]
    pub(in crate::compilation) fn declared_unit_keys_for_test(
        &self,
    ) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        self.declared_unit_keys()
    }

    fn push_primary_unit_key(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        owner: SymbolKey,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        let anchor = declaration.syntax_anchor();

        if callable_body_kind(declaration.kind()) && syntax.has_callable_body(anchor) {
            push_key(
                keys,
                BoundUnitKey::callable_body(owner, self.bound_source(anchor)?),
            )?;

            return Ok(());
        }

        let constructor = match declaration.kind() {
            DeclarationKind::Constant | DeclarationKind::TraitConstantMember => {
                BoundUnitKey::constant_template
            }
            DeclarationKind::Predicate | DeclarationKind::TraitPredicateMember => {
                BoundUnitKey::predicate_definition
            }
            _ => return Ok(()),
        };

        let Some(expression) = syntax.first_expression(anchor) else {
            return Ok(());
        };

        push_key(keys, constructor(owner, self.bound_source(expression)?))
    }

    fn push_surface_unit_keys(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        symbol: AnySymbolId,
        owner: SymbolKey,
        symbols: &SymbolGraph,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        for anchor in declaration.surface().constraints() {
            if syntax.has_bound_constraint_expression(*anchor) {
                push_key(
                    keys,
                    BoundUnitKey::constraint(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        for anchor in declaration.surface().contract_clauses() {
            if syntax.has_bound_constraint_expression(*anchor) {
                push_key(
                    keys,
                    BoundUnitKey::contract_clause(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        let Some(default) = declaration.surface().runtime_default() else {
            return Ok(());
        };

        let Some(expression) = syntax.first_expression(default) else {
            return Ok(());
        };

        let provider = symbols
            .runtime_default_provider(symbol)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Synthesized provider keys are Arc-backed and retained by the runtime-default unit key.
        let provider = symbols
            .symbol_key(provider)
            .cloned()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        push_key(
            keys,
            BoundUnitKey::runtime_default(provider, self.bound_source(expression)?),
        )
    }

    fn semantic_unit_target_validity(
        &self,
        unit: &BoundUnit,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let (types, selections, _) = semantics;

        let values = self.semantic_value_store()?;
        let mut diagnostics = DiagnosticBag::new();

        for entry in types.entries() {
            cancellation.check()?;

            if entry.result().is_recovered() {
                continue;
            }

            let data = values
                .type_data(entry.result().ty())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
                continue;
            };

            let bray_symbols::NamedTypeSymbolId::Struct(definition) = definition else {
                continue;
            };

            let Some(role) = self
                .available_compiler_known_symbols()
                .provider()
                .role_registry()
                .symbol_representation(*definition)
            else {
                continue;
            };

            let expression = unit
                .view()
                .expression(entry.expression())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let request = TargetValidityRequest::new(
                expression.origin().source_anchor(),
                TargetValidityRequirement::Representation(role),
            );

            let result = self.target_validity_with_cancellation(request, cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());
        }

        for entry in selections.entries() {
            cancellation.check()?;

            let SemanticSelection::Call(call) = entry.selection() else {
                continue;
            };

            if call.abi() == bray_symbols::CallableAbi::Bray {
                continue;
            }

            let Some(BoundExpression::Call(expression)) =
                unit.view().expression(entry.expression())
            else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            let callee = types
                .expression(expression.callee())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let data = values
                .type_data(callee.ty())
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    super::foreign::target_abi_value_from_type(self, parameter.ty(), cancellation)
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten();

            let result =
                super::foreign::target_abi_value_from_type(self, callable.result(), cancellation)?;

            let request = TargetValidityRequest::new(
                expression.origin().source_anchor(),
                TargetValidityRequirement::CallableAbi(TargetCallableAbiRequirement::new(
                    call.abi(),
                    parameters,
                    result,
                )),
            );

            let result = self.target_validity_with_cancellation(request, cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());
        }

        Ok(diagnostics)
    }
}

enum SemanticDiagnosticSource {
    Bound(Arc<DiagnosticResult<BoundUnit>>),
    DeclaredTypes(Arc<DiagnosticResult<DeclaredValueTypeTemplates>>),
    EmbeddedConstants(DiagnosticResult<bray_checker::CheckedConstantTerms>),
    ExpressionSemantics(Arc<PublishedUnitFact<CheckedExpressionSemantics>>),
    ControlFlow(Arc<DiagnosticResult<CheckedControlFlow>>),
    Patterns(Arc<DiagnosticResult<CheckedPatterns>>),
    Storage(Arc<DiagnosticResult<StoragePlan>>),
    Liveness(Arc<DiagnosticResult<Liveness>>),
    Refinements(Arc<DiagnosticResult<CheckedRefinements>>),
    StorageFlow(Arc<DiagnosticResult<StorageFlow>>),
    Dependencies(Arc<DiagnosticResult<CheckedDependencyContracts>>),
    Memory(Arc<DiagnosticResult<CheckedMemoryOperations>>),
    Async(Arc<DiagnosticResult<CheckedAsync>>),
    BodyBehavior(Arc<DiagnosticResult<CheckedBodyBehavior>>),
    TargetValidity(DiagnosticBag),
    ConstantTemplate(Arc<DiagnosticResult<ConstantDefinitionState>>),
    ConstantInstance(Arc<DiagnosticResult<bray_checker::EvaluatedConstantCall>>),
    ModuleSurface(Arc<DiagnosticResult<ModuleSurface>>),
    CallableContracts(Arc<SymbolFactResult<CallableContractsQuery>>),
    TypeRepresentation(Arc<DiagnosticResult<DeclaredTypeRepresentation>>),
    TraitConformance(Arc<SemanticFactResult<TraitImplementationConformanceQuery>>),
}

impl SemanticDiagnosticSource {
    fn diagnostics(&self) -> &DiagnosticBag {
        match self {
            Self::Bound(result) => result.diagnostics(),
            Self::DeclaredTypes(result) => result.diagnostics(),
            Self::EmbeddedConstants(result) => result.diagnostics(),
            Self::ExpressionSemantics(result) => result.result().diagnostics(),
            Self::ControlFlow(result) => result.diagnostics(),
            Self::Patterns(result) => result.diagnostics(),
            Self::Storage(result) => result.diagnostics(),
            Self::Liveness(result) => result.diagnostics(),
            Self::Refinements(result) => result.diagnostics(),
            Self::StorageFlow(result) => result.diagnostics(),
            Self::Dependencies(result) => result.diagnostics(),
            Self::Memory(result) => result.diagnostics(),
            Self::Async(result) => result.diagnostics(),
            Self::BodyBehavior(result) => result.diagnostics(),
            Self::TargetValidity(diagnostics) => diagnostics,
            Self::ConstantTemplate(result) => result.diagnostics(),
            Self::ConstantInstance(result) => result.diagnostics(),
            Self::ModuleSurface(result) => result.diagnostics(),
            Self::CallableContracts(result) => result.diagnostics(),
            Self::TypeRepresentation(result) => result.diagnostics(),
            Self::TraitConformance(result) => result.diagnostics(),
        }
    }
}

struct SemanticSyntaxIndex {
    entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry>,
}

impl SemanticSyntaxIndex {
    fn new<'declaration>(
        syntax: &SyntaxTree,
        declarations: impl IntoIterator<Item = &'declaration DeclarationRecord>,
    ) -> Self {
        let mut entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry> = HashMap::new();

        for declaration in declarations {
            entries.entry(declaration.syntax_anchor()).or_default();

            for anchor in declaration
                .surface()
                .constraints()
                .iter()
                .chain(declaration.surface().contract_clauses())
                .copied()
                .chain(declaration.surface().runtime_default())
            {
                entries.entry(anchor).or_default();
            }
        }

        let mut active = Vec::new();

        walk_syntax_tree(syntax, |event| {
            let node = match event {
                SyntaxWalkEvent::EnterNode(node) => node,
                SyntaxWalkEvent::ExitNode(node) => {
                    let anchor = SyntaxAnchor::from_node(&node);

                    if active.last() == Some(&anchor) {
                        active.pop();
                    }

                    return SyntaxWalkControl::Continue;
                }
                SyntaxWalkEvent::Token(_) => return SyntaxWalkControl::Continue,
            };

            let anchor = SyntaxAnchor::from_node(&node);

            if entries.contains_key(&anchor) {
                active.push(anchor);
            }

            if node.kind() == SyntaxKind::CallableBodyBlockExpression
                && let Some(active_anchor) = active.last()
                && let Some(entry) = entries.get_mut(active_anchor)
            {
                entry.has_callable_body = true;
            }

            for active_anchor in &active {
                let Some(entry) = entries.get_mut(active_anchor) else {
                    continue;
                };

                if node.kind() == SyntaxKind::Expression && entry.first_expression.is_none() {
                    entry.first_expression = Some(anchor);

                    entry.first_expression_is_trait_satisfaction =
                        node.cast::<ExpressionSyntax>().is_some_and(|expression| {
                            expression.trait_satisfaction_constraint().is_some()
                        });
                }
            }

            SyntaxWalkControl::Continue
        });

        Self { entries }
    }

    fn has_callable_body(&self, anchor: SyntaxAnchor) -> bool {
        self.entries
            .get(&anchor)
            .is_some_and(|entry| entry.has_callable_body)
    }

    fn has_bound_constraint_expression(&self, anchor: SyntaxAnchor) -> bool {
        self.entries.get(&anchor).is_some_and(|entry| {
            entry.first_expression.is_some() && !entry.first_expression_is_trait_satisfaction
        })
    }

    fn first_expression(&self, anchor: SyntaxAnchor) -> Option<SyntaxAnchor> {
        self.entries.get(&anchor)?.first_expression
    }
}

#[derive(Default)]
struct SemanticSyntaxEntry {
    first_expression: Option<SyntaxAnchor>,
    first_expression_is_trait_satisfaction: bool,
    has_callable_body: bool,
}

fn callable_body_kind(kind: DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::Function
            | DeclarationKind::TraitCallableMember
            | DeclarationKind::TypeConstructorMember
            | DeclarationKind::FinalizerMember
            | DeclarationKind::DestructorMember
            | DeclarationKind::ScopeEnterMember
            | DeclarationKind::ScopeExitMember
            | DeclarationKind::TypeCallableMember
    )
}

fn push_key(keys: &mut Vec<BoundUnitKey>, key: Option<BoundUnitKey>) -> Result<(), FactQueryError> {
    let key = key.ok_or(FactQueryError::InfrastructureFailure)?;

    keys.push(key);

    Ok(())
}

fn unit_order_key(
    key: BoundUnitKey,
) -> (
    bray_source::SourceId,
    bray_source::TextRange,
    BoundUnitKind,
    BoundUnitKey,
) {
    let source = key.source().syntax();

    (source.source_id(), source.full_range(), key.kind(), key)
}

#[cfg(test)]
mod tests {
    use bray_binder::semantic_unit_context;
    use bray_bound_tree::{BoundUnitKind, BoundUnitRoot};
    use bray_checker::SemanticUnitContext;
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{CallableContractClauseKind, ConstantTermData};

    use crate::WorkerBudget;
    use crate::fact::FactCellTestEvent;
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget, diagnostic_kinds,
    };

    #[test]
    fn package_semantic_diagnostics_request_all_declared_unit_categories() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "predicate valid() = true;\n",
            "struct value with(true)\n",
            "{\n",
            "}\n",
            "func check(value: i32 = 1) requires(true) ensures(true) with(true)\n",
            "{\n",
            "}\n",
        ));

        let mut kinds = match compilation.declared_unit_keys() {
            Ok(keys) => keys.into_iter().map(|key| key.kind()).collect::<Vec<_>>(),
            Err(error) => panic!("semantic unit keys must be discoverable: {error:?}"),
        };

        kinds.sort_unstable();

        assert_eq!(
            kinds,
            [
                BoundUnitKind::CallableBody,
                BoundUnitKind::RuntimeDefault,
                BoundUnitKind::ConstantTemplate,
                BoundUnitKind::PredicateDefinition,
                BoundUnitKind::Constraint,
                BoundUnitKind::ContractClause,
                BoundUnitKind::ContractClause,
                BoundUnitKind::ContractClause,
            ]
        );
    }

    #[test]
    fn check_diagnostics_lazily_request_and_cache_semantics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let value = 1;\n",
            "    let value = 2;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("callable key must be discoverable: {error:?}"),
        };

        let [key] = keys.as_slice() else {
            panic!("test package must contain one semantic unit");
        };

        assert_eq!(compilation.state.bound_units.is_published(key), Ok(false));

        assert_eq!(
            compilation.state.checked_control_flow.is_published(key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(key),
            Ok(false)
        );

        assert_eq!(compilation.state.liveness.is_published(key), Ok(false));

        assert_eq!(
            compilation.state.dependency_contracts.is_published(key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.checked_body_behaviors.is_published(key),
            Ok(false)
        );

        let first = compilation.check_diagnostics();
        let second = compilation.check_diagnostics();

        assert!(std::ptr::eq(first, second));
        assert_eq!(compilation.state.bound_units.is_published(key), Ok(true));

        assert_eq!(
            compilation.state.checked_control_flow.is_published(key),
            Ok(true)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(key),
            Ok(true)
        );

        assert_eq!(compilation.state.liveness.is_published(key), Ok(true));

        assert_eq!(
            compilation.state.dependency_contracts.is_published(key),
            Ok(true)
        );

        assert_eq!(
            compilation.state.checked_body_behaviors.is_published(key),
            Ok(true)
        );

        assert_eq!(
            diagnostic_kinds(first),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn check_diagnostics_include_expression_target_validity() {
        let compilation = compilation(
            r#"module app;

func main(value: r16)
{
    value;
}
"#,
        );

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingTargetRepresentationUnavailable)
        );
    }

    #[test]
    fn check_diagnostics_request_embedded_constant_expressions() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos source: [i32; false])\n",
            "{\n",
            "    let copy: [i32; false] = source;\n",
            "}\n",
        ));

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("callable unit must be discoverable: {error:?}"));

        let [key] = keys.as_slice() else {
            panic!("test source must produce one callable unit");
        };

        let declared = compilation
            .declared_value_type_templates(key.clone())
            .unwrap_or_else(|error| panic!("declared types must publish: {error:?}"));

        let occurrence = declared
            .value()
            .evidence()
            .iter()
            .flat_map(|evidence| evidence.template().constant_expressions())
            .next()
            .unwrap_or_else(|| panic!("array type must retain its length expression"));

        let checked = compilation
            .embedded_constant_term(occurrence)
            .unwrap_or_else(|error| panic!("embedded constant must recover: {error:?}"));

        assert!(!checked.diagnostics().is_empty());

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingIncompatibleExpressionType)
        );
    }

    #[test]
    fn bare_generic_constant_arguments_bind_as_embedded_values() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Buffer<const size: usize>\n",
            "{\n",
            "}\n",
            "func use_buffer<const count: usize>(pos value: Buffer<count>)\n",
            "{\n",
            "}\n",
        ));

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("callable unit must be discoverable: {error:?}"));

        let key = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::CallableBody)
            .unwrap_or_else(|| panic!("test source must produce one callable body"));

        let declared = compilation
            .declared_value_type_templates(key)
            .unwrap_or_else(|error| panic!("declared types must publish: {error:?}"));

        let occurrence = declared
            .value()
            .evidence()
            .iter()
            .flat_map(|evidence| evidence.template().constant_expressions())
            .next()
            .unwrap_or_else(|| panic!("generic argument must retain its constant expression"));

        let checked = compilation
            .embedded_constant_term(occurrence)
            .unwrap_or_else(|error| panic!("embedded constant must bind: {error:?}"));

        let term = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"))
            .constant_term_data(*checked.value())
            .unwrap_or_else(|error| panic!("embedded constant term must resolve: {error:?}"));

        assert!(checked.diagnostics().is_empty());

        assert!(matches!(term.as_ref(), ConstantTermData::Parameter(_)));
    }

    #[test]
    fn malformed_calls_to_known_functions_publish_diagnostics_without_panicking() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func known(pos value: i32)\n",
            "{\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    known(value = );\n",
            "}\n",
        ));

        assert!(!compilation.check_diagnostics().is_empty());
    }

    #[test]
    fn package_diagnostics_include_nested_unit_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda()\n",
            "    {\n",
            "        let value = 1;\n",
            "        let value = 2;\n",
            "    };\n",
            "}\n",
        ));

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn package_diagnostics_bind_every_contract_clause_expression() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func check() requires(true, missing)\n",
            "{\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
        };

        let Some(key) = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::ContractClause)
        else {
            panic!("test source must produce a contract-clause key");
        };

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::BindingUnresolvedName],
            "{:#?}",
            compilation.check_diagnostics()
        );

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("contract clause must bind: {error:?}"),
        };

        let BoundUnitRoot::ExpressionSequence(root) = bound.value().root() else {
            panic!("contract clause must publish an expression sequence");
        };

        let Some(sequence) = bound.value().tree().block(root) else {
            panic!("contract-clause sequence root must resolve");
        };

        assert_eq!(sequence.items().len(), 2);
    }

    #[test]
    fn postcondition_result_reaches_the_semantic_unit_context() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func check() -> i32 ensures(result == 1)\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
        };

        let Some(key) = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::ContractClause)
        else {
            panic!("test source must produce a contract-clause key");
        };

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("contract clause must bind: {error:?}"),
        };

        assert!(bound.diagnostics().is_empty());

        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let entry = match semantic_unit_context(symbols, bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("semantic unit context must be available: {error:?}"),
        };

        let SemanticUnitContext::ContractClause(entry) = entry else {
            panic!("contract clause must produce a contract-clause checker entry");
        };

        let [result] = bound.value().local_symbols().postcondition_results() else {
            panic!("value-producing postcondition must declare one result symbol");
        };

        assert_eq!(entry.kind(), CallableContractClauseKind::Ensures);
        assert_eq!(entry.result(), Some(result.id()));
    }

    #[test]
    fn contract_clause_entries_preserve_kind_and_exact_result_availability() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func omitted() ensures(true)\n",
            "{\n",
            "}\n",
            "func unit_result() -> unit ensures(true)\n",
            "{\n",
            "}\n",
            "func never_result() -> never ensures(true)\n",
            "{\n",
            "}\n",
            "func value_result() -> i32 requires(true) ensures(result == 1) with(true)\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause keys must be discoverable: {error:?}"),
        };

        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let mut entries = Vec::new();

        for key in keys
            .into_iter()
            .filter(|key| key.kind() == BoundUnitKind::ContractClause)
        {
            let source_start = key.source().syntax().full_range().start();

            let bound = match compilation.bound_unit(key) {
                Ok(bound) => bound,
                Err(error) => panic!("contract clause must bind: {error:?}"),
            };

            assert!(bound.diagnostics().is_empty());

            let entry = match semantic_unit_context(symbols, bound.value()) {
                Ok(entry) => entry,
                Err(error) => panic!("semantic unit context must be available: {error:?}"),
            };

            let SemanticUnitContext::ContractClause(entry) = entry else {
                panic!("contract clause must produce a contract-clause checker entry");
            };

            entries.push((source_start, entry.kind(), entry.result().is_some()));
        }

        entries.sort_unstable_by_key(|entry| entry.0);

        assert_eq!(
            entries
                .into_iter()
                .map(|(_, kind, has_result)| (kind, has_result))
                .collect::<Vec<_>>(),
            [
                (CallableContractClauseKind::Ensures, false),
                (CallableContractClauseKind::Ensures, false),
                (CallableContractClauseKind::Ensures, false),
                (CallableContractClauseKind::Requires, false),
                (CallableContractClauseKind::Ensures, true),
                (CallableContractClauseKind::Static, false),
            ]
        );
    }

    #[test]
    fn declarations_without_semantic_units_do_not_force_binding() {
        let compilation = compilation(concat!(
            "module app;\n",
            "extern func write(value: i32);\n",
            "trait Writer\n",
            "{\n",
            "    func flush();\n",
            "}\n",
        ));

        assert!(
            compilation
                .declared_unit_keys()
                .is_ok_and(|keys| keys.is_empty())
        );

        assert!(compilation.check_diagnostics().is_empty());
    }

    #[test]
    fn malformed_bodies_publish_recovered_semantics_without_panicking() {
        let cases = [
            concat!(
                "module app;\n",
                "func missing_initializer()\n",
                "{\n",
                "    let value = ;\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "func malformed_pattern()\n",
                "{\n",
                "    let (first, second = 1;\n",
                "}\n",
            ),
        ];

        for source in cases {
            let compilation = compilation(source);

            let keys = match compilation.declared_unit_keys() {
                Ok(keys) => keys,
                Err(error) => panic!("recovered unit keys must be discoverable: {error:?}"),
            };

            let mut recovered = false;

            assert!(!keys.is_empty(), "{source}");

            for key in keys {
                let checked = match compilation.control_flow(key) {
                    Ok(checked) => checked,
                    Err(error) => panic!("recovered semantic check failed: {source}: {error:?}"),
                };

                recovered |= checked.value().is_recovered();
            }

            assert!(recovered, "{source}");
            assert!(!compilation.check_diagnostics().is_empty(), "{source}");
        }
    }

    #[test]
    fn concurrent_package_diagnostic_requests_publish_one_cached_result() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    missing;\n",
            "}\n",
        ));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .check_diagnostics
            .set_test_observer(gate.observer())
        {
            panic!("package diagnostic source must accept a test observer: {error:?}");
        }

        let diagnostics = std::thread::scope(|scope| {
            let owner = scope.spawn(|| compilation.check_diagnostics());

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter = scope.spawn(|| compilation.check_diagnostics());

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            [owner, waiter].map(|handle| match handle.join() {
                Ok(diagnostics) => diagnostics,
                Err(_) => panic!("package diagnostic request panicked"),
            })
        });

        assert!(
            diagnostics
                .iter()
                .skip(1)
                .all(|result| std::ptr::eq(diagnostics[0], *result))
        );

        assert_eq!(
            diagnostic_kinds(diagnostics[0]),
            [DiagnosticKind::BindingUnresolvedName]
        );
    }

    #[test]
    fn semantic_diagnostics_ignore_serial_parallel_and_reversed_demand_order() {
        let sources = [
            concat!(
                "module app;\n",
                "func first()\n",
                "{\n",
                "    let value = 1;\n",
                "    let value = 2;\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "func second()\n",
                "{\n",
                "    missing;\n",
                "}\n",
            ),
        ];

        let serial = compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

        let serial_keys = match serial.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("serial unit keys must be discoverable: {error:?}"),
        };

        for key in serial_keys {
            if let Err(error) = serial.control_flow(key) {
                panic!("serial semantic demand failed: {error:?}");
            }
        }

        let parallel_budget = match WorkerBudget::new(4) {
            Ok(budget) => budget,
            Err(error) => panic!("parallel test budget must be valid: {error:?}"),
        };

        let parallel = compilation_with_sources_and_worker_budget(&sources, parallel_budget);

        let mut parallel_keys = match parallel.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("parallel unit keys must be discoverable: {error:?}"),
        };

        parallel_keys.reverse();

        assert_eq!(parallel_keys.len(), 2);

        let gates = parallel_keys
            .iter()
            .map(|key| {
                let gate = FactTestGate::holding(FactCellTestEvent::Computing);

                if let Err(error) = parallel
                    .state
                    .checked_control_flow
                    .set_test_observer(key, gate.observer())
                {
                    panic!("control-flow source must accept a test observer: {error:?}");
                }

                gate
            })
            .collect::<Vec<_>>();

        // Bound-unit keys share immutable identity storage across worker requests.
        std::thread::scope(|scope| {
            let parallel = &parallel;

            let handles = parallel_keys
                .into_iter()
                .map(|key| scope.spawn(move || parallel.control_flow(key)))
                .collect::<Vec<_>>();

            for gate in &gates {
                gate.wait_until_observed(FactCellTestEvent::Computing, 1);
            }

            for (gate, handle) in gates.iter().zip(handles) {
                gate.release();

                match handle.join() {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => panic!("parallel semantic demand failed: {error:?}"),
                    Err(_) => panic!("parallel semantic demand panicked"),
                }
            }
        });

        assert_eq!(serial.check_diagnostics(), parallel.check_diagnostics());

        assert_eq!(
            diagnostic_kinds(serial.check_diagnostics()),
            [
                DiagnosticKind::BindingNameAlreadyDefined,
                DiagnosticKind::BindingUnresolvedName,
            ]
        );
    }
}
