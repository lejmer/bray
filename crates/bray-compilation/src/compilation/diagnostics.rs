// rust-style: allow(module-too-large, reason = "semantic diagnostic aggregation shares one cached publication boundary")

use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{
    AnyBoundNodeId, BoundBlock, BoundCallableBody, BoundExpression, BoundPattern, BoundUnit,
    BoundUnitKey, BoundUnitKind, CheckedExpressionSemantics, SelectedArgument, SemanticSelection,
};
use bray_checker::{
    TargetAbiValue, TargetCallableAbiRequirement, TargetValidityRequest, TargetValidityRequirement,
};
use bray_compiler_known::ImplementationHook;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticEmissionEvaluationFailure,
    DiagnosticEmissionFailure, DiagnosticId, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSymbolReference, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticProductKind, DiagnosticResult, SeverityKind,
};
use bray_package_interface::InterfaceSymbolReference;
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableContractSet, CallableContractsQuery, CallableDefinitionId,
    CallableSymbolId, ImplementationSymbolId, ImportedSymbolSkeleton, ModuleSurfaceQuery,
    NamedTypeSymbolId, ProductKind, SymbolGraph, SymbolOrigin, SymbolQueryKind, SymbolQueryRequest,
    diagnostic_external_symbol_identity, diagnostic_symbol_identity, diagnostic_symbol_kind,
};

use super::binder::has_visible_generic_parameters;
use super::constant::constant_definition_id;
use super::state::Compilation;
use super::{
    ProductQueryFailure, ProductValueKind, SemanticDataKind, SemanticQueryContext,
    SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::{
    BatchWork, CancellationToken, CompilationFactKey, DiagnosticPublicationOrder, FactQueryError,
    OrderedDiagnosticCollection, PublishedUnitResult, SymbolQueryKey, publish_diagnostics,
};

#[derive(Hash)]
pub(in crate::compilation) struct SemanticDiagnosticPublication {
    diagnostics: DiagnosticBag,
    callable_contracts: BTreeMap<CallableSymbolId, Arc<DiagnosticResult<CallableContractSet>>>,
}

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

pub(super) fn with_compiler_defect_source(
    diagnostic: Diagnostic,
    source: SourceSpan,
) -> Diagnostic {
    diagnostic
        .with_primary_span(source)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::CompilerDefectSource,
            source,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ))
}

pub(super) fn with_compiler_defect_note(diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.with_note(DiagnosticNote::new(
        DiagnosticNoteKind::ReportCompilerDefect,
    ))
}

fn checker_failure_diagnostics(
    key: &BoundUnitKey,
    bound: Option<&BoundUnit>,
    error: bray_checker::CheckerInfrastructureError,
) -> DiagnosticBag {
    let anchor = key.source().syntax();

    let source = checker_failure_source(&error, bound)
        .unwrap_or_else(|| SourceSpan::new(anchor.source_id(), anchor.full_range()));

    let failure =
        DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::Checker(
            crate::fact::diagnostic_checker_failure(error),
        ));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        DiagnosticKind::CheckingCompilerDefect,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::emission_failure(failure));

    DiagnosticBag::single(with_compiler_defect_source(diagnostic, source))
}

fn semantic_query_failure_diagnostics(
    key: &BoundUnitKey,
    error: &crate::compilation::SemanticQueryError,
) -> DiagnosticBag {
    let anchor = key.source().syntax();

    let source = error
        .source()
        .unwrap_or_else(|| SourceSpan::new(anchor.source_id(), anchor.full_range()));

    let failure =
        DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::SemanticQuery(
            crate::fact::diagnostic_semantic_query_failure(error),
        ));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        DiagnosticKind::CheckingCompilerDefect,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::emission_failure(failure));

    DiagnosticBag::single(with_compiler_defect_source(diagnostic, source))
}

fn checker_failure_source(
    error: &bray_checker::CheckerInfrastructureError,
    bound: Option<&BoundUnit>,
) -> Option<SourceSpan> {
    use bray_checker::{
        CheckedConstantTermsBuildError, CheckerConstantEvaluationFailure,
        CheckerInfrastructureError as Error,
    };

    match error {
        Error::CheckedConstantTerms(CheckedConstantTermsBuildError::DuplicateOccurrence(key)) => {
            let syntax = key.syntax();

            Some(SourceSpan::new(syntax.source_id(), syntax.full_range()))
        }
        Error::ConstantEvaluation(CheckerConstantEvaluationFailure::MissingPatternBinding {
            binding,
        }) => bound.and_then(|bound| local_symbol_source(bound, (*binding).into())),
        _ => checker_failure_node(error)
            .and_then(|node| bound.and_then(|bound| bound_node_source(bound, node))),
    }
}

fn checker_failure_node(
    error: &bray_checker::CheckerInfrastructureError,
) -> Option<AnyBoundNodeId> {
    use bray_checker::{
        CheckerConstantEvaluationFailure as ConstantEvaluation,
        CheckerInfrastructureError as Error, CheckerLiteralValueFailure as Literal,
        CheckerStorageFlowFailure as Flow,
    };

    match error {
        Error::InvalidStorageOperation { expression, .. } => Some((*expression).into()),
        Error::LiteralValue(failure) => match failure {
            Literal::InvalidLiteral { expression }
            | Literal::MissingExpressionType { expression }
            | Literal::MissingLiteralValue { expression }
            | Literal::ValueTypeMismatch { expression }
            | Literal::DuplicateExpression { expression } => Some((*expression).into()),
            Literal::ForeignExpressionTypes => None,
        },
        Error::ConstantEvaluation(failure) => match failure {
            ConstantEvaluation::InvalidExpressionRoot { expression }
            | ConstantEvaluation::MissingExpressionType { expression }
            | ConstantEvaluation::MissingExpression { expression } => Some((*expression).into()),
            ConstantEvaluation::InvalidBlockRoot { block }
            | ConstantEvaluation::MissingBlockResultType { block }
            | ConstantEvaluation::MissingBlock { block } => Some((*block).into()),
            ConstantEvaluation::MissingPatternInput { pattern }
            | ConstantEvaluation::MissingPattern { pattern } => Some((*pattern).into()),
            ConstantEvaluation::MissingPatternBinding { .. }
            | ConstantEvaluation::UnexpectedPropagation { .. } => None,
        },
        Error::StorageFlow(failure) => match failure {
            Flow::MissingAwaitDependencyContract { expression }
            | Flow::MissingDependencyContract { expression, .. } => Some((*expression).into()),
            Flow::MissingExitOrigin { exit } => Some(*exit),
            Flow::MissingBlock { block }
            | Flow::UnbalancedScopes {
                open_scope: Some(block),
            } => Some((*block).into()),
            Flow::MissingPattern { pattern } => Some((*pattern).into()),
            _ => None,
        },
        _ => None,
    }
}

fn local_symbol_source(
    bound: &BoundUnit,
    local: bray_symbols::AnyLocalSymbolId,
) -> Option<SourceSpan> {
    let anchor = bound.local_symbols().syntax_anchor(local)?;

    Some(SourceSpan::new(anchor.source_id(), anchor.full_range()))
}

fn bound_node_source(bound: &BoundUnit, node: AnyBoundNodeId) -> Option<SourceSpan> {
    let view = bound.view();

    let origin = match node {
        AnyBoundNodeId::Expression(expression) => {
            view.expression(expression).map(BoundExpression::origin)
        }
        AnyBoundNodeId::Pattern(pattern) => view.pattern(pattern).map(BoundPattern::origin),
        AnyBoundNodeId::Block(block) => view.block(block).map(BoundBlock::origin),
        AnyBoundNodeId::CallableBody(body) => view
            .callable_body(body)
            .copied()
            .map(BoundCallableBody::origin),
    }?;

    let anchor = origin.source_anchor().syntax();

    Some(SourceSpan::new(anchor.source_id(), anchor.full_range()))
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
        .ok_or_else(|| {
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(symbol),
                SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
            ))
        })?;

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

pub(super) fn diagnostic_interface_symbol_reference(
    reference: &InterfaceSymbolReference,
) -> DiagnosticInterfaceSymbolReference {
    match reference {
        InterfaceSymbolReference::Local(symbol) => {
            DiagnosticInterfaceSymbolReference::Local(symbol.raw())
        }
        InterfaceSymbolReference::Dependency { dependency, key } => {
            DiagnosticInterfaceSymbolReference::Dependency {
                dependency: dependency.raw(),
                identity: diagnostic_external_symbol_identity(key),
            }
        }
        InterfaceSymbolReference::CompilerKnown(reference) => {
            DiagnosticInterfaceSymbolReference::CompilerKnown(diagnostic_symbol_identity(
                reference.key(),
            ))
        }
    }
}

impl Compilation {
    /// Returns diagnostics produced by binding and semantic analysis of this package.
    pub fn semantic_diagnostics(&self) -> &DiagnosticBag {
        super::boundary::expect_uncancelled_query(
            "semantic_diagnostics",
            self.semantic_diagnostics_with_cancellation(&self.state.cancellation),
        )
    }

    pub(super) fn semantic_diagnostics_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_with_cancellation(
            CompilationFactKey::SemanticDiagnostics,
            &self.state.semantic_diagnostics,
            cancellation,
            |cancellation| self.compute_semantic_diagnostics(cancellation),
        )
        .map(|publication| &publication.diagnostics)
    }

    pub(in crate::compilation) fn published_callable_contract(
        &self,
        callable: CallableSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Option<Arc<DiagnosticResult<CallableContractSet>>>, FactQueryError> {
        let Some(publication) = self
            .state
            .semantic_diagnostics
            .get_if_published(&CompilationFactKey::SemanticDiagnostics)
        else {
            return Ok(None);
        };

        let Some(contracts) = publication.callable_contracts.get(&callable) else {
            return Ok(None);
        };

        let key = CompilationFactKey::from(SymbolQueryKey::new(
            callable.into_any(),
            SymbolQueryKind::CallableContracts,
        ));

        self.state.fact_runtime.check_request_cycle(&key)?;
        cancellation.check()?;
        self.state.fact_runtime.record_completed_request(&key)?;

        Ok(Some(Arc::clone(contracts)))
    }

    /// Returns diagnostics for the current whole-package check request.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        super::boundary::expect_uncancelled_query(
            "check_diagnostics",
            self.check_diagnostics_with_cancellation(&self.state.cancellation),
        )
    }

    pub(super) fn check_diagnostics_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_with_cancellation(
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

            Ok::<_, FactQueryError>(diagnostics)
        })?;

        let diagnostics = diagnostics.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(publish_diagnostics(
            self.state.fact_runtime.profile(),
            ordered_diagnostic_collections(diagnostics),
        ))
    }

    fn has_incomplete_callable_parameters(&self) -> bool {
        if !self.syntax_tree_result().diagnostics().has_errors() {
            return false;
        }

        let mut incomplete = false;

        bray_syntax::walk_syntax_tree(self.syntax_tree(), |event| {
            if let bray_syntax::SyntaxWalkEvent::EnterNode(node) = event
                && let Some(parameter) = node.cast::<bray_syntax::ParameterSyntax>()
                && parameter.identifier_token().is_missing()
            {
                incomplete = true;

                return bray_syntax::SyntaxWalkControl::Stop;
            }

            bray_syntax::SyntaxWalkControl::Continue
        });

        incomplete
    }

    fn compute_semantic_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<SemanticDiagnosticPublication, FactQueryError> {
        // Recovered nameless parameters cannot form complete callable signatures.
        // Other syntax recovery can still publish useful semantic diagnostics.
        if self.source_diagnostics().has_errors() || self.has_incomplete_callable_parameters() {
            return Ok(SemanticDiagnosticPublication {
                diagnostics: DiagnosticBag::new(),
                callable_contracts: BTreeMap::new(),
            });
        }

        let source_graph = self.product_source_graph()?;

        let symbols = self.symbol_graph()?;

        // Publication owns diagnostic bags after their analysis results are released.
        let mut sources = Vec::new();
        let binder = self.binding_context(cancellation)?;

        for module in symbols
            .modules()
            .iter()
            .filter(|module| module.origin() == SymbolOrigin::Source)
        {
            let surface = binder
                .resolve_symbol_query(SymbolQueryRequest::<ModuleSurfaceQuery>::new(module.id()))
                .map_err(super::binder::binding_query_error)?;

            sources.push(surface.diagnostics().clone());
        }

        let mut callables_without_bodies = Vec::new();
        let mut callable_order = BTreeMap::new();
        let contract_start = sources.len();
        let mut checked_contracts = BTreeMap::new();

        for (ordinal, callable) in source_graph
            .declarations()
            .declarations()
            .iter()
            .filter_map(|declaration| symbols.symbol_for_declaration(declaration.id()))
            .filter_map(CallableSymbolId::try_from_any)
            .enumerate()
        {
            callable_order.insert(callable, ordinal);
            sources.push(DiagnosticBag::new());

            let body = match CallableDefinitionId::try_new(callable.into_any()) {
                Some(definition) => self.callable_body_key(definition)?,
                None => None,
            };

            if body.is_none() {
                callables_without_bodies.push((ordinal, callable));
            }
        }

        let contract_results =
            self.state
                .fact_runtime
                .map_indexed(callables_without_bodies.len(), |index| {
                    cancellation.check()?;

                    let (ordinal, callable) = callables_without_bodies[index];

                    let contracts = binder
                        .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                            callable,
                        ))
                        .map_err(super::binder::binding_query_error)?;

                    // Each worker returns its bag without retaining the callable analysis.
                    Ok::<_, FactQueryError>((ordinal, callable, contracts))
                })?;

        for contracts in contract_results {
            let (ordinal, callable, contracts) = contracts?;

            sources[contract_start + ordinal] = contracts.diagnostics().clone();
            checked_contracts.insert(callable, contracts);
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
            sources.push(
                self.declared_type_representation(subject)?
                    .diagnostics()
                    .clone(),
            );
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
            let coherence = binder
                .resolve_symbol_query(SymbolQueryRequest::<
                    bray_symbols::ImplementationCoherenceQuery,
                >::new(implementation))
                .map_err(super::binder::binding_query_error)?;

            if coherence.diagnostics().has_errors() {
                // Diagnostic publication owns this source bag after the query result is released.
                sources.push(coherence.diagnostics().clone());

                continue;
            }

            sources.push(
                self.trait_implementation_conformance(implementation)?
                    .diagnostics()
                    .clone(),
            );
        }

        // Scheduled diagnostics retain shared identities independently of the inventory.
        let roots = self
            .declared_unit_keys()?
            .iter()
            .cloned()
            .map(unit_order_key);

        let mut units = self
            .state
            .fact_runtime
            .complete_batch(roots, cancellation, |(_, _, _, key)| {
                // Contract and unit diagnostics reuse this worker's checked body before eviction.
                let contracts = if key.kind() == BoundUnitKind::CallableBody {
                    let callable = symbols
                        .symbol_for_key(key.declared_owner())
                        .and_then(CallableSymbolId::try_from_any)
                        .expect("declared callable body must have a callable owner");

                    let contracts = binder
                        .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                            callable,
                        ))
                        .map_err(super::binder::binding_query_error)?;

                    let ordinal = *callable_order
                        .get(&callable)
                        .expect("declared callable body must have a diagnostic publication order");

                    Some((ordinal, callable, contracts))
                } else {
                    None
                };

                let (sources, nested) = self.semantic_unit_diagnostic_work(key, cancellation)?;

                Ok::<_, FactQueryError>(BatchWork::new((contracts, sources), nested))
            })
            .map_err(|error| error.into_fact_query_error())?;

        units.sort_unstable_by(|left, right| left.0.cmp(&right.0));

        for (_, (contracts, unit_sources)) in units {
            if let Some((ordinal, callable, contracts)) = contracts {
                sources[contract_start + ordinal] = contracts.diagnostics().clone();
                checked_contracts.insert(callable, contracts);
            }

            sources.extend(unit_sources);
        }

        let coherence = self.implementation_coherence_diagnostics(cancellation)?;
        let execution_guarantees = self.execution_guarantee_diagnostics(source_graph)?;
        let callable_overloads = self.callable_overload_diagnostics(cancellation)?;
        let foreign_callables = self.foreign_callable_diagnostics(cancellation)?;
        let product = self.product_semantics_with_cancellation(cancellation)?;

        let collections = ordered_diagnostic_collections(
            std::iter::once(source_graph.diagnostics())
                .chain(sources.iter())
                .chain([
                    coherence,
                    &execution_guarantees,
                    callable_overloads,
                    foreign_callables,
                    product.diagnostics(),
                ]),
        );

        Ok(SemanticDiagnosticPublication {
            diagnostics: publish_diagnostics(self.state.fact_runtime.profile(), collections),
            callable_contracts: checked_contracts,
        })
    }

    fn semantic_unit_diagnostic_work(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<(Vec<DiagnosticBag>, Vec<UnitDiagnosticOrder>), FactQueryError> {
        let (bound, sources) = match self
            .semantic_unit_diagnostic_sources(key.clone(), cancellation)
        {
            Ok((bound, sources)) => (Some(bound), sources),
            Err(
                error @ (FactQueryError::CheckerInfrastructure(_)
                | FactQueryError::SemanticQuery(_)),
            ) => {
                let bound = self
                    .bound_unit_with_cancellation(key.clone(), cancellation)
                    .ok();

                let diagnostic = match error {
                    FactQueryError::CheckerInfrastructure(error) => checker_failure_diagnostics(
                        key,
                        bound.as_ref().map(|bound| bound.result().value()),
                        error,
                    ),
                    FactQueryError::SemanticQuery(error) => {
                        semantic_query_failure_diagnostics(key, &error)
                    }
                    _ => {
                        unreachable!("recoverable unit failure must be a checker or semantic error")
                    }
                };

                (bound, vec![diagnostic])
            }
            Err(error) => return Err(error),
        };

        let nested = bound
            .iter()
            .flat_map(|bound| bound.result().value().nested_units().iter().cloned())
            .map(unit_order_key)
            .collect();

        Ok((sources, nested))
    }

    pub(super) fn semantic_unit_diagnostics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let (_, sources) = self.semantic_unit_diagnostic_sources(key, cancellation)?;

        Ok(publish_diagnostics(
            self.state.fact_runtime.profile(),
            ordered_diagnostic_collections(sources.iter()),
        ))
    }

    fn semantic_unit_diagnostic_sources(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<(Arc<PublishedUnitResult<BoundUnit>>, Vec<DiagnosticBag>), FactQueryError> {
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
        let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
        let body_semantics = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

        let target_validity = self.semantic_unit_target_validity(
            bound.result().value(),
            expression_semantics.result().value(),
            cancellation,
        )?;

        // The batch retains bags without extending the lifetime of checked values.
        let mut sources = vec![
            bound.result().diagnostics().clone(),
            declared_types.result().diagnostics().clone(),
            embedded_constants.diagnostics().clone(),
            expression_semantics.result().diagnostics().clone(),
            control_flow.result().diagnostics().clone(),
            patterns.result().diagnostics().clone(),
            storage.result().diagnostics().clone(),
            memory.result().diagnostics().clone(),
            body_semantics.result().diagnostics().clone(),
            behavior.result().diagnostics().clone(),
            target_validity,
        ];

        if key.kind() == BoundUnitKind::ConstantTemplate {
            let symbols = self.symbol_graph()?;

            let owner = symbols
                .symbol_for_key(key.declared_owner())
                .ok_or_else(|| {
                    FactQueryError::from(SemanticQueryFailure::contract(
                        SemanticQueryContext::SymbolKey(key.declared_owner().clone()),
                        SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                    ))
                })?;

            if let AnySymbolId::Static(declaration) = owner {
                sources.push(
                    self.static_instance_template(declaration)?
                        .diagnostics()
                        .clone(),
                );

                return Ok((bound, sources));
            }

            let definition = constant_definition_id(owner).ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::Missing(SemanticDataKind::ConstantDefinition),
                ))
            })?;

            let template = self.constant_definition(definition)?;

            sources.push(template.diagnostics().clone());

            if !has_visible_generic_parameters(symbols, owner) {
                let substitution = crate::compilation::substitution::empty_substitution(
                    self.semantic_value_store()?,
                    definition.into_any(),
                )?;

                let instance =
                    bray_symbols::ConstantInstanceKey::new(definition, substitution, None);

                let value = self.constant_instance_with_cancellation(instance, cancellation)?;

                sources.push(value.diagnostics().clone());
            }
        }

        Ok((bound, sources))
    }

    fn semantic_unit_target_validity(
        &self,
        unit: &BoundUnit,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let types = semantics.types();
        let selections = semantics.selections();

        let values = self.semantic_value_store()?;
        let mut diagnostics = DiagnosticBag::new();

        for entry in types.entries() {
            cancellation.check()?;

            if entry.result().is_recovered() {
                continue;
            }

            let data = values.type_data(entry.result().ty());

            let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
                continue;
            };

            let NamedTypeSymbolId::Struct(definition) = definition else {
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

            let expression = unit.view().expression(entry.expression()).ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::BoundExpression {
                        unit: unit.unit(),
                        expression: entry.expression(),
                    },
                    SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
                ))
            })?;

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
                return Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::BoundExpression {
                        unit: unit.unit(),
                        expression: entry.expression(),
                    },
                    SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
                )
                .into());
            };

            let callee = types.expression(expression.callee()).ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::BoundExpression {
                        unit: unit.unit(),
                        expression: expression.callee(),
                    },
                    SemanticQueryViolation::Missing(SemanticDataKind::Type),
                ))
            })?;

            let data = values.type_data(callee.ty());

            let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                return Err(ProductQueryFailure::UnexpectedSemanticType {
                    ty: callee.ty(),
                    expected: ProductValueKind::CallableType,
                    actual: data.as_ref().clone(),
                }
                .into());
            };

            let mut parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    super::foreign::target_abi_value_from_type(self, parameter.ty(), cancellation)
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|value| value.unwrap_or(TargetAbiValue::Unsupported))
                .collect::<Vec<_>>();

            if call.implementation_hook() == Some(ImplementationHook::NativeThreadExecution)
                && let Some(operation) = parameters.first_mut()
            {
                *operation = TargetAbiValue::RawPointer;
            }

            for argument in call.arguments() {
                let SelectedArgument::Explicit {
                    ordinal,
                    conversion,
                    ..
                } = argument
                else {
                    continue;
                };

                if usize::try_from(*ordinal)
                    .is_ok_and(|ordinal| ordinal < callable.parameters().len())
                {
                    continue;
                }

                let value = super::foreign::target_abi_value_from_type(
                    self,
                    conversion.target_type(),
                    cancellation,
                )?
                .unwrap_or(TargetAbiValue::Unsupported);

                parameters.push(value);
            }

            let result =
                super::foreign::target_abi_value_from_type(self, callable.result(), cancellation)?;

            let request = TargetValidityRequest::new(
                expression.origin().source_anchor(),
                TargetValidityRequirement::CallableAbi(
                    TargetCallableAbiRequirement::new(call.abi(), parameters, result)
                        .with_variadic(callable.is_variadic()),
                ),
            );

            let result = self.target_validity_with_cancellation(request, cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());
        }

        Ok(diagnostics)
    }
}

fn ordered_diagnostic_collections<'diagnostic>(
    diagnostics: impl IntoIterator<Item = &'diagnostic DiagnosticBag>,
) -> Vec<OrderedDiagnosticCollection> {
    diagnostics
        .into_iter()
        .enumerate()
        .map(|(ordinal, diagnostics)| {
            OrderedDiagnosticCollection::new(DiagnosticPublicationOrder::new(ordinal), diagnostics)
        })
        .collect()
}

type UnitDiagnosticOrder = (
    bray_source::SourceId,
    bray_source::TextRange,
    BoundUnitKind,
    BoundUnitKey,
);

fn unit_order_key(key: BoundUnitKey) -> UnitDiagnosticOrder {
    let source = key.source().syntax();

    (source.source_id(), source.full_range(), key.kind(), key)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::semantic_unit_context;
    use bray_bound_tree::{BoundUnitKind, BoundUnitRoot};
    use bray_checker::SemanticUnitContext;
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_messages::{DiagnosticRenderer, RenderedDiagnostic};
    use bray_source::TextSize;
    use bray_symbols::{
        CallableContractClauseKind, ConstantTermData, PackageIdentity, ProductKind,
    };
    use bray_target::NativeTarget;
    use bray_testing::assert_goal_state_diagnostic_kind;

    use crate::fact::{CancellationToken, FactCellTestEvent};
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget, diagnostic_kinds,
        source_callable_body_key, source_input,
    };
    use crate::{
        Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
    };

    #[test]
    fn diagnostics_share_body_work_beyond_the_query_cache_working_set() {
        use std::fmt::Write;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use bray_symbols::SymbolQueryKind;

        use crate::fact::{CompilationFactKey, FactEvaluationTestObserver};
        use crate::test_support::source_function_body_key;

        let count = 4_100;
        let mut source = String::from("module app;\n");

        for index in 0..count {
            writeln!(source, "func item{index}() {{}}").unwrap();
        }

        let compilation = compilation(&source);
        let bodies = Arc::new(AtomicUsize::new(0));
        let contracts = Arc::new(AtomicUsize::new(0));
        let observed_bodies = Arc::clone(&bodies);
        let observed_contracts = Arc::clone(&contracts);

        compilation
            .state
            .fact_runtime
            .set_test_observer(FactEvaluationTestObserver::new(move |key| {
                if matches!(key, CompilationFactKey::BodySemantics(_)) {
                    observed_bodies.fetch_add(1, Ordering::SeqCst);
                }

                if matches!(key, CompilationFactKey::Symbol(query)
                    if query.kind() == SymbolQueryKind::CallableContracts)
                {
                    observed_contracts.fetch_add(1, Ordering::SeqCst);
                }
            }))
            .unwrap();

        assert!(compilation.check_diagnostics().is_empty());
        assert_eq!(bodies.load(Ordering::SeqCst), count);
        assert_eq!(contracts.load(Ordering::SeqCst), count);

        let first = source_function_body_key(&compilation, "item0");
        let last = source_function_body_key(&compilation, "item4099");

        assert_eq!(
            compilation.state.body_semantics.is_published(&first),
            Ok(false)
        );

        assert_eq!(
            compilation.state.body_semantics.is_published(&last),
            Ok(true)
        );
    }

    #[test]
    fn unit_diagnostic_bags_do_not_retain_checked_body_results() {
        let compilation = compilation(
            r#"
            module app;

            func main()
            {
            }
            "#,
        );

        let key = source_callable_body_key(&compilation);
        let cancellation = CancellationToken::new();

        let initial = compilation
            .semantic_unit_diagnostic_sources(key.clone(), &cancellation)
            .expect("unit diagnostics must initialize the analysis caches");

        drop(initial);

        let body = compilation
            .body_semantics_with_cancellation(key.clone(), &cancellation)
            .expect("checked body must remain cached");

        let owners = Arc::strong_count(&body);

        let (_, diagnostics) = compilation
            .semantic_unit_diagnostic_sources(key, &cancellation)
            .expect("unit diagnostics must remain available");

        assert!(diagnostics.iter().all(DiagnosticBag::is_empty));
        assert_eq!(Arc::strong_count(&body), owners);
    }

    #[test]
    fn checker_infrastructure_failures_publish_the_compiler_defect_diagnostic() {
        let compilation = compilation("module app; func main() {}");
        let key = source_callable_body_key(&compilation);

        let diagnostics = super::checker_failure_diagnostics(
            &key,
            None,
            bray_checker::CheckerInfrastructureError::SemanticValueUnavailable,
        );

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::CheckingCompilerDefect);
    }

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

        let mut kinds = match compilation.declared_unit_keys_for_test() {
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
    fn static_directive_arguments_are_not_initializer_units() {
        let source = concat!(
            "module app;\n",
            "@symbol(name = \"exported_value\")\n",
            "static EXPORTED_VALUE: i32 = 41;\n",
        );

        let compilation = compilation(source);

        let keys = compilation
            .declared_unit_keys_for_test()
            .unwrap_or_else(|error| panic!("static initializer key must be available: {error:?}"));

        let [key] = keys.as_slice() else {
            panic!("test package must contain one semantic unit");
        };

        let initializer_start = source
            .find("41")
            .and_then(|offset| u32::try_from(offset).ok())
            .map(TextSize::from);

        assert_eq!(
            Some(key.source().syntax().full_range().start()),
            initializer_start
        );
    }

    #[test]
    fn static_initializers_do_not_prevent_constant_pattern_analysis() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static LOCK_STATE: u32 = 0;\n",
            "const ONE: u32 = 1;\n",
            "func is_one(pos value: u32) -> bool\n",
            "{\n",
            "    return match value\n",
            "    {\n",
            "        case ONE { yield true; }\n",
            "        case _ { yield false; }\n",
            "    };\n",
            "}\n",
        ));

        let diagnostics = compilation
            .semantic_diagnostics_with_cancellation(&compilation.state.cancellation)
            .unwrap_or_else(|error| {
                panic!("static and constant diagnostics must be available: {error:?}")
            });

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn target_gated_nested_module_values_resolve_across_source_units() {
        let sources = [
            concat!(
                "@target(target.identity.NAME == \"x86_64-unknown-linux-gnu\")\n",
                "trusted module std.os.linux;\n",
                "const FLAG: u32 = 1;\n",
                "extern trusted func native_value() -> u32 uses(foreign_call);\n",
            ),
            concat!(
                "@target(target.identity.NAME == \"x86_64-unknown-linux-gnu\")\n",
                "trusted module std.platform;\n",
                "using std.os.linux;\n",
                "trusted func value() -> u32\n",
                "{\n",
                "    return trusted std.os.linux.native_value() + std.os.linux.FLAG;\n",
                "}\n",
            ),
        ];

        let compilation =
            compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn static_atomic_storage_uses_constant_initialization() {
        let source = concat!(
            "module app;\n",
            "static INPUT_LOCK: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);\n",
            "static OUTPUT_LOCK: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);\n",
            "static ERROR_LOCK: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);\n",
        );

        let package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard library package identity must be valid"));

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        );

        let request =
            CompilationRequest::with_options(package, vec![source_input(source, 0)], options)
                .with_standard_library_source_authority();

        let compilation = Compilation::load(request).unwrap_or_else(|error| {
            panic!("standard library test compilation must load: {error:?}")
        });

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("atomic static symbols must publish: {error:?}"));

        for declaration in symbols.statics().iter().map(bray_symbols::StaticSymbol::id) {
            let template = compilation
                .static_instance_template(declaration)
                .unwrap_or_else(|error| panic!("atomic static template must publish: {error:?}"));

            assert!(
                template.diagnostics().is_empty(),
                "{:#?}",
                template.diagnostics()
            );
        }
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

        let keys = match compilation.declared_unit_keys_for_test() {
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

        assert_eq!(
            compilation.state.body_semantics.is_published(key),
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

        assert_eq!(compilation.state.body_semantics.is_published(key), Ok(true));

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
            .declared_unit_keys_for_test()
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
            .declared_unit_keys_for_test()
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
            .constant_term_data(*checked.value());

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

        let keys = match compilation.declared_unit_keys_for_test() {
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

        let keys = match compilation.declared_unit_keys_for_test() {
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

        let entry = semantic_unit_context(symbols, bound.value());

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

        let keys = match compilation.declared_unit_keys_for_test() {
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

            let entry = semantic_unit_context(symbols, bound.value());

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
                .declared_unit_keys_for_test()
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

            let keys = match compilation.declared_unit_keys_for_test() {
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

        let serial_keys = match serial.declared_unit_keys_for_test() {
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

        let mut parallel_keys = match parallel.declared_unit_keys_for_test() {
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
            rendered_diagnostics(serial.check_diagnostics()),
            rendered_diagnostics(parallel.check_diagnostics())
        );

        assert_eq!(
            diagnostic_kinds(serial.check_diagnostics()),
            [
                DiagnosticKind::BindingNameAlreadyDefined,
                DiagnosticKind::BindingUnresolvedName,
            ]
        );
    }

    fn rendered_diagnostics(diagnostics: &DiagnosticBag) -> Vec<RenderedDiagnostic> {
        diagnostics
            .iter()
            .map(|diagnostic| DiagnosticRenderer::english().render(diagnostic))
            .collect()
    }
}
