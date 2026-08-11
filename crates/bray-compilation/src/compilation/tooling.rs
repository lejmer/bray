// rust-style: allow(module-too-large, reason = "source-correlated semantic queries share one syntax-to-bound lookup implementation")

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundSourceAnchor, BoundUnit,
    BoundUnitKey, ExpressionTypeResult, SelectedCall, SemanticSelection,
};
use bray_declarations::{DeclarationId, DeclarationRecord, DeclarationTable, SyntaxAnchor};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::{SourceId, TextRange, TextSize};
use bray_symbols::{AnySymbolId, SymbolKind, SymbolOrigin};
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_node};

use super::facts::Compilation;
use crate::fact::{CancellationToken, FactQueryError, QueryPriority};

type BoundUnitFact = Arc<DiagnosticResult<BoundUnit>>;
type SyntaxBoundExpression = (BoundUnitFact, BoundExpressionId);
type SyntaxBoundUnit = (BoundUnitKey, BoundUnitFact);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct SourceReferenceIndex {
    references: BTreeMap<BoundReferenceTarget, Vec<BoundSourceAnchor>>,
}

/// Availability of one source-correlated semantic answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticAvailability<T> {
    /// Semantic analysis produced an ordinary answer.
    Available(T),
    /// Recovery affected the answer, which may still retain a usable value.
    Recovered(Option<T>),
    /// The requested semantic provider has no answer for this syntax.
    Unavailable,
}

/// One ordinary name visible to completion lookup at a source position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionCandidate {
    name: String,
    kind: SymbolKind,
}

impl CompletionCandidate {
    /// Returns the visible ordinary name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the semantic category of the visible symbol.
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }
}

impl Compilation {
    /// Returns the innermost syntax node covering a source position.
    pub fn syntax_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Option<BoundSourceAnchor>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let Some(source) = self.source(source_id) else {
                return Ok(None);
            };

            let Some(syntax) = self.source_unit_syntax(source_id) else {
                return Ok(None);
            };

            cancellation.check()?;

            let mut result = None;
            let mut is_root = true;

            walk_syntax_node(syntax.source_unit(), |event| {
                let SyntaxWalkEvent::EnterNode(node) = event else {
                    return SyntaxWalkControl::Continue;
                };

                let range = node.full_range();
                let covers_position = range_covers_position(range, position);
                let covers_end_of_source = is_root && range.end() == position;

                is_root = false;

                if !covers_position && !covers_end_of_source {
                    return SyntaxWalkControl::SkipChildren;
                }

                if covers_position || covers_end_of_source {
                    result = Some(SyntaxAnchor::from_node(&node));
                }

                SyntaxWalkControl::Continue
            });

            Ok(result.map(|syntax| BoundSourceAnchor::new(syntax, source.version())))
        })
    }

    /// Returns the innermost declaration covering a source position.
    pub fn declaration_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<DeclarationId>, FactQueryError> {
        let Some(syntax) = self.syntax_at(source_id, position, cancellation, priority)? else {
            return Ok(SemanticAvailability::Unavailable);
        };

        self.declaration_for_syntax(syntax, cancellation, priority)
    }

    /// Returns the innermost declaration containing a syntax identity.
    pub fn declaration_for_syntax(
        &self,
        source: BoundSourceAnchor,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<DeclarationId>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let Some(syntax) = self.current_syntax(source, cancellation)? else {
                return Ok(SemanticAvailability::Unavailable);
            };

            let Some(declaration) = declaration_for_syntax(self.declaration_table(), syntax) else {
                return Ok(recovery_or_unavailable(syntax.is_recovered(), None));
            };

            Ok(availability(
                syntax.is_recovered() || declaration.is_recovered(),
                declaration.id(),
            ))
        })
    }

    /// Returns the semantic reference or declaration at a source position.
    pub fn symbol_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<BoundReferenceTarget>, FactQueryError> {
        let Some(syntax) = self.syntax_at(source_id, position, cancellation, priority)? else {
            return Ok(SemanticAvailability::Unavailable);
        };

        self.symbol_for_syntax(syntax, cancellation, priority)
    }

    /// Returns the semantic reference or declaration associated with a syntax identity.
    pub fn symbol_for_syntax(
        &self,
        source: BoundSourceAnchor,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<BoundReferenceTarget>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let Some(syntax) = self.current_syntax(source, cancellation)? else {
                return Ok(SemanticAvailability::Unavailable);
            };

            self.symbol_for_current_syntax(syntax, cancellation)
        })
    }

    /// Returns the source definition reached from a source position.
    pub fn definition_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<BoundSourceAnchor>, FactQueryError> {
        let Some(syntax) = self.syntax_at(source_id, position, cancellation, priority)? else {
            return Ok(SemanticAvailability::Unavailable);
        };

        self.definition_for_syntax(syntax, cancellation, priority)
    }

    /// Returns the source definition reached from a syntax identity.
    pub fn definition_for_syntax(
        &self,
        source: BoundSourceAnchor,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<BoundSourceAnchor>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let Some(syntax) = self.current_syntax(source, cancellation)? else {
                return Ok(SemanticAvailability::Unavailable);
            };

            let symbol = self.symbol_for_current_syntax(syntax, cancellation)?;

            Ok(match symbol {
                SemanticAvailability::Available(symbol) => self
                    .definition_for_symbol(symbol, syntax, cancellation)?
                    .map(SemanticAvailability::Available)
                    .unwrap_or(SemanticAvailability::Unavailable),
                SemanticAvailability::Recovered(symbol) => {
                    let definition = match symbol {
                        Some(symbol) => self.definition_for_symbol(symbol, syntax, cancellation)?,
                        None => None,
                    };

                    SemanticAvailability::Recovered(definition)
                }
                SemanticAvailability::Unavailable => SemanticAvailability::Unavailable,
            })
        })
    }

    /// Returns every source occurrence that resolves to the symbol at a position.
    pub fn references_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<Vec<BoundSourceAnchor>>, FactQueryError> {
        let target = match self.symbol_at(source_id, position, cancellation, priority)? {
            SemanticAvailability::Available(target) => target,
            SemanticAvailability::Recovered(Some(target)) => {
                return self
                    .references_to(target, cancellation, priority)
                    .map(|references| SemanticAvailability::Recovered(Some(references)));
            }
            SemanticAvailability::Recovered(None) => {
                return Ok(SemanticAvailability::Recovered(None));
            }
            SemanticAvailability::Unavailable => {
                return Ok(SemanticAvailability::Unavailable);
            }
        };

        self.references_to(target, cancellation, priority)
            .map(SemanticAvailability::Available)
    }

    /// Returns the checked expression type at a source position.
    pub fn expression_type_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<ExpressionTypeResult>, FactQueryError> {
        let Some(syntax) = self.syntax_at(source_id, position, cancellation, priority)? else {
            return Ok(SemanticAvailability::Unavailable);
        };

        self.expression_type_for_syntax(syntax, cancellation, priority)
    }

    /// Returns the checked expression type associated with a syntax identity.
    pub fn expression_type_for_syntax(
        &self,
        source: BoundSourceAnchor,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<ExpressionTypeResult>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let Some(syntax) = self.current_syntax(source, cancellation)? else {
                return Ok(SemanticAvailability::Unavailable);
            };

            let Some((bound, expression)) =
                self.bound_expression_for_syntax(syntax, cancellation)?
            else {
                return Ok(recovery_or_unavailable(syntax.is_recovered(), None));
            };

            // The semantic fact owns its Arc-backed unit key independently of `bound`.
            let key = bound.value().key().clone();
            let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

            let Some(result) = semantics.result().value().0.expression(expression) else {
                return Ok(recovery_or_unavailable(true, None));
            };

            Ok(availability(
                syntax.is_recovered() || result.is_recovered(),
                result,
            ))
        })
    }

    /// Returns the innermost independently checked bound unit covering a source position.
    pub fn bound_unit_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Option<Arc<DiagnosticResult<BoundUnit>>>, FactQueryError> {
        let Some(source) = self.syntax_at(source_id, position, cancellation, priority)? else {
            return Ok(None);
        };

        self.run_semantic_query(cancellation, priority, || {
            self.bound_unit_for_syntax(source.syntax(), cancellation)
                .map(|bound| bound.map(|(_, result)| result))
        })
    }

    /// Returns every independently checked bound unit rooted in one source.
    pub fn bound_units_for_source(
        &self,
        source_id: SourceId,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Vec<BoundUnitFact>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let mut keys = self
                .declared_unit_keys()?
                .into_iter()
                .filter(|key| key.source().syntax().source_id() == source_id)
                .collect::<Vec<_>>();

            keys.sort_by_key(|key| {
                let source = key.source().syntax();

                (
                    source.full_range().start(),
                    source.full_range().end(),
                    key.kind(),
                )
            });

            let mut units = Vec::new();

            for key in keys {
                units.extend(self.bound_unit_family_with_cancellation(key, cancellation)?);
            }

            Ok(units)
        })
    }

    /// Returns source-load, syntax, and declaration diagnostics for one source.
    pub fn diagnostics_for_source(
        &self,
        source_id: SourceId,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<DiagnosticBag, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            // The scoped result owns diagnostics independently of the compilation cache.
            let source_diagnostics = self
                .source_diagnostics()
                .iter()
                .filter(|diagnostic| {
                    diagnostic
                        .primary_span()
                        .is_some_and(|span| span.source_id() == source_id)
                })
                .cloned()
                .collect::<DiagnosticBag>();

            let syntax = self.source_unit_syntax(source_id);
            let declarations = self.declaration_chunk(source_id);
            let mut bags = vec![&source_diagnostics];

            if let Some(syntax) = syntax {
                bags.push(syntax.diagnostics());
            }

            if let Some(declarations) = declarations {
                bags.push(declarations.diagnostics());
            }

            Ok(DiagnosticBag::merged_all(bags))
        })
    }

    /// Returns all semantic diagnostics owned by one bound unit.
    pub fn diagnostics_for_unit(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<DiagnosticBag, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            self.semantic_unit_diagnostics_with_cancellation(key, cancellation)
        })
    }

    /// Returns diagnostics for a complete package check.
    pub fn diagnostics_for_package(
        &self,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<DiagnosticBag, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            // The returned bag remains owned after the package fact cache is released.
            let diagnostics = self
                .check_diagnostics_with_cancellation(cancellation)?
                .clone();

            Ok(diagnostics)
        })
    }

    fn run_semantic_query<T>(
        &self,
        cancellation: &CancellationToken,
        priority: QueryPriority,
        query: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        cancellation.check()?;

        self.state.fact_runtime.run(priority, || {
            cancellation.check()?;

            let result = query()?;

            cancellation.check()?;

            Ok(result)
        })
    }

    /// Returns declaration-backed names visible from one source position.
    pub fn completion_candidates_at(
        &self,
        source_id: SourceId,
        position: TextSize,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Vec<CompletionCandidate>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let declaration =
                match self.declaration_at(source_id, position, cancellation, priority)? {
                    SemanticAvailability::Available(declaration)
                    | SemanticAvailability::Recovered(Some(declaration)) => Some(declaration),
                    SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {
                        None
                    }
                };

            let graph = self.symbol_graph()?;
            let mut owners = Vec::new();

            if let Some(symbol) =
                declaration.and_then(|declaration| graph.symbol_for_declaration(declaration))
            {
                let mut current = Some(symbol);

                while let Some(symbol) = current {
                    owners.push(symbol);
                    current = graph.containing_symbol(symbol);
                }
            }

            let mut candidates = BTreeMap::<String, SymbolKind>::new();

            for symbol in graph.symbols() {
                cancellation.check()?;

                let origin = graph.symbol_origin(symbol);

                let is_visible_member = graph
                    .containing_symbol(symbol)
                    .is_some_and(|owner| owners.contains(&owner))
                    && is_accessible_completion_symbol(graph, symbol, origin);

                let is_global_module = matches!(symbol, AnySymbolId::Module(_))
                    && matches!(
                        origin,
                        Some(SymbolOrigin::Imported | SymbolOrigin::CompilerKnown)
                    )
                    && graph
                        .symbol_visibility(symbol)
                        .is_none_or(|visibility| visibility.is_public());

                if !is_visible_member && !is_global_module {
                    continue;
                }

                let Some(name) = graph.member_name(symbol) else {
                    continue;
                };

                candidates
                    .entry(name.as_str().to_owned())
                    .or_insert(symbol.kind());
            }

            Ok(candidates
                .into_iter()
                .map(|(name, kind)| CompletionCandidate { name, kind })
                .collect())
        })
    }

    /// Returns the checked call selected for one call-operation syntax identity.
    pub fn selected_call_for_syntax(
        &self,
        source: BoundSourceAnchor,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SemanticAvailability<SelectedCall>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let Some(syntax) = self.current_syntax(source, cancellation)? else {
                return Ok(SemanticAvailability::Unavailable);
            };

            let Some((_, bound)) = self.bound_unit_for_syntax(syntax, cancellation)? else {
                return Ok(recovery_or_unavailable(syntax.is_recovered(), None));
            };

            let selections = self
                .semantic_selections_with_cancellation(bound.value().key().clone(), cancellation)?;

            let selected = bound
                .value()
                .tree()
                .expressions()
                .filter(|(_, expression)| {
                    syntax_contains(expression.origin().source_anchor().syntax(), syntax)
                })
                .filter_map(|(expression, node)| {
                    let SemanticSelection::Call(call) =
                        selections.result().value().expression(expression)?
                    else {
                        return None;
                    };

                    Some((
                        expression,
                        node,
                        node.origin().source_anchor().syntax(),
                        call,
                    ))
                })
                .min_by(|left, right| compare_syntax_ranges(left.2, right.2));

            let Some((_, expression, _, call)) = selected else {
                return Ok(recovery_or_unavailable(syntax.is_recovered(), None));
            };

            let recovered = syntax.is_recovered() || expression.is_recovered();

            Ok(availability(recovered, call.clone()))
        })
    }

    fn references_to(
        &self,
        target: BoundReferenceTarget,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Vec<BoundSourceAnchor>, FactQueryError> {
        self.run_semantic_query(cancellation, priority, || {
            let mut references = Vec::new();

            for source in self.sources() {
                cancellation.check()?;

                if let Some(index) =
                    self.source_reference_index(source.source_id(), cancellation, priority)?
                {
                    references.extend(index.references.get(&target).into_iter().flatten().copied());
                }
            }

            references.sort_by_key(|reference| {
                let syntax = reference.syntax();

                (
                    syntax.source_id(),
                    syntax.full_range().start(),
                    syntax.full_range().end(),
                    syntax.syntax_kind(),
                )
            });

            references.dedup_by(|left, right| {
                left.syntax().source_id() == right.syntax().source_id()
                    && left.syntax().full_range() == right.syntax().full_range()
            });

            Ok(references)
        })
    }

    fn source_reference_index(
        &self,
        source_id: SourceId,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Option<&SourceReferenceIndex>, FactQueryError> {
        let Some(index) = source_id.to_index() else {
            return Ok(None);
        };

        let Some(cell) = self.state.source_reference_indexes.get(index) else {
            return Ok(None);
        };

        cell.get_or_compute_with_priority(
            &self.state.fact_runtime,
            crate::fact::CompilationFactKey::SourceReferenceIndex(source_id),
            cancellation,
            priority,
            || self.build_source_reference_index(source_id, cancellation, priority),
        )
        .map(Some)
    }

    fn build_source_reference_index(
        &self,
        source_id: SourceId,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<SourceReferenceIndex, FactQueryError> {
        let source = self
            .source(source_id)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let graph = self.symbol_graph()?;
        let declarations = self.product_source_graph()?.declarations();
        let mut references = BTreeMap::<BoundReferenceTarget, Vec<BoundSourceAnchor>>::new();

        for declaration in declarations
            .declarations()
            .iter()
            .filter(|declaration| declaration.source_id() == source_id)
        {
            cancellation.check()?;

            let Some(symbol) = graph.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            push_reference(
                &mut references,
                BoundReferenceTarget::Surface(symbol),
                BoundSourceAnchor::new(declaration.syntax_anchor(), source.version()),
            );
        }

        for bound in self.bound_units_for_source(source_id, cancellation, priority)? {
            cancellation.check()?;

            let unit = bound.value();

            for symbol in unit
                .local_symbols()
                .bindings()
                .iter()
                .map(|symbol| symbol.id().into())
                .chain(
                    unit.local_symbols()
                        .constants()
                        .iter()
                        .map(|symbol| symbol.id().into()),
                )
                .chain(
                    unit.local_symbols()
                        .anonymous_callables()
                        .iter()
                        .map(|symbol| symbol.id().into()),
                )
                .chain(
                    unit.local_symbols()
                        .anonymous_parameters()
                        .iter()
                        .map(|symbol| symbol.id().into()),
                )
                .chain(
                    unit.local_symbols()
                        .postcondition_results()
                        .iter()
                        .map(|symbol| symbol.id().into()),
                )
            {
                let Some(syntax) = unit.local_symbols().syntax_anchor(symbol) else {
                    continue;
                };

                push_reference(
                    &mut references,
                    BoundReferenceTarget::Local(symbol),
                    BoundSourceAnchor::new(syntax, source.version()),
                );
            }

            let has_pattern_references = unit
                .tree()
                .expressions()
                .any(|(_, expression)| matches!(expression, BoundExpression::PatternReference(_)));

            let selections = if has_pattern_references {
                Some(self.semantic_selections_with_cancellation(unit.key().clone(), cancellation)?)
            } else {
                None
            };

            for (expression_id, expression) in unit.tree().expressions() {
                cancellation.check()?;

                let target = match expression {
                    BoundExpression::Name(reference) => Some(reference.target()),
                    BoundExpression::PatternReference(_) => selections
                        .as_ref()
                        .and_then(|selections| {
                            selections.result().value().expression(expression_id)
                        })
                        .and_then(|selection| match selection {
                            SemanticSelection::Reference(target) => Some(*target),
                            _ => None,
                        }),
                    _ => None,
                };

                if let Some(target) = target {
                    push_reference(&mut references, target, expression.origin().source_anchor());
                }
            }
        }

        for anchors in references.values_mut() {
            anchors.sort_by_key(reference_sort_key);
            anchors.dedup();
        }

        Ok(SourceReferenceIndex { references })
    }

    fn bound_expression_for_syntax(
        &self,
        syntax: SyntaxAnchor,
        cancellation: &CancellationToken,
    ) -> Result<Option<SyntaxBoundExpression>, FactQueryError> {
        let Some((_, bound)) = self.bound_unit_for_syntax(syntax, cancellation)? else {
            return Ok(None);
        };

        let expression = expression_for_syntax(bound.value(), syntax);

        Ok(expression.map(|expression| (bound, expression)))
    }

    fn current_syntax(
        &self,
        source: BoundSourceAnchor,
        cancellation: &CancellationToken,
    ) -> Result<Option<SyntaxAnchor>, FactQueryError> {
        let syntax = source.syntax();

        let Some(snapshot) = self.source(syntax.source_id()) else {
            return Ok(None);
        };

        if snapshot.version() != source.source_version() {
            return Ok(None);
        }

        let Some(source_unit) = self.source_unit_syntax(syntax.source_id()) else {
            return Ok(None);
        };

        let mut found = false;

        walk_syntax_node(source_unit.source_unit(), |event| {
            if cancellation.is_cancelled() {
                return SyntaxWalkControl::Stop;
            }

            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            let anchor = SyntaxAnchor::from_node(&node);

            if anchor == syntax {
                found = true;

                return SyntaxWalkControl::Stop;
            }

            if !syntax_contains(anchor, syntax) {
                return SyntaxWalkControl::SkipChildren;
            }

            SyntaxWalkControl::Continue
        });

        cancellation.check()?;

        Ok(found.then_some(syntax))
    }

    fn symbol_for_current_syntax(
        &self,
        syntax: SyntaxAnchor,
        cancellation: &CancellationToken,
    ) -> Result<SemanticAvailability<BoundReferenceTarget>, FactQueryError> {
        let declarations = self.product_source_graph()?.declarations();

        if let Some(declaration) = declaration_at_syntax(declarations, syntax) {
            let Some(symbol) = self
                .symbol_graph()?
                .symbol_for_declaration(declaration.id())
            else {
                return Ok(recovery_or_unavailable(
                    syntax.is_recovered() || declaration.is_recovered(),
                    None,
                ));
            };

            return Ok(availability(
                syntax.is_recovered() || declaration.is_recovered(),
                BoundReferenceTarget::Surface(symbol),
            ));
        }

        let Some((_, bound)) = self.bound_unit_for_syntax(syntax, cancellation)? else {
            return Ok(recovery_or_unavailable(syntax.is_recovered(), None));
        };

        if let Some(expression) = expression_for_syntax(bound.value(), syntax) {
            return self.expression_symbol(bound.value(), expression, syntax, cancellation);
        }

        let local = bound
            .value()
            .local_symbols()
            .symbol_for_syntax(syntax)
            .map(BoundReferenceTarget::Local);

        Ok(recovery_or_unavailable(syntax.is_recovered(), local))
    }

    fn bound_unit_for_syntax(
        &self,
        syntax: SyntaxAnchor,
        cancellation: &CancellationToken,
    ) -> Result<Option<SyntaxBoundUnit>, FactQueryError> {
        let keys = self.declared_unit_keys()?;

        let Some(key) = keys
            .iter()
            .filter(|key| syntax_contains(key.source().syntax(), syntax))
            .min_by(|left, right| {
                compare_syntax_ranges(left.source().syntax(), right.source().syntax())
            })
        else {
            return Ok(None);
        };

        // The query must retain the selected Arc-backed unit identity after dropping `keys`.
        let mut key = key.clone();

        loop {
            // The result retains `key` while the fact request owns its Arc-backed clone.
            let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

            let nested = bound
                .result()
                .value()
                .nested_units()
                .iter()
                .filter(|nested| syntax_contains(nested.source().syntax(), syntax))
                .min_by(|left, right| {
                    compare_syntax_ranges(left.source().syntax(), right.source().syntax())
                });

            let Some(nested) = nested else {
                return Ok(Some((key, Arc::clone(bound.result()))));
            };

            // The next request owns the selected Arc-backed nested identity.
            key = nested.clone();
        }
    }

    fn expression_symbol(
        &self,
        bound: &BoundUnit,
        expression: BoundExpressionId,
        syntax: SyntaxAnchor,
        cancellation: &CancellationToken,
    ) -> Result<SemanticAvailability<BoundReferenceTarget>, FactQueryError> {
        let Some(node) = bound.view().expression(expression) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let target = match node {
            BoundExpression::Name(reference) => Some(reference.target()),
            BoundExpression::PatternReference(_) => {
                // The semantic fact owns its Arc-backed unit key independently of `bound`.
                let key = bound.key().clone();

                let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

                match semantics.result().value().1.expression(expression) {
                    Some(SemanticSelection::Reference(target)) => Some(*target),
                    _ => None,
                }
            }
            _ => None,
        };

        Ok(recovery_or_unavailable(
            syntax.is_recovered() || node.is_recovered(),
            target,
        ))
    }

    fn definition_for_symbol(
        &self,
        symbol: BoundReferenceTarget,
        syntax: SyntaxAnchor,
        cancellation: &CancellationToken,
    ) -> Result<Option<BoundSourceAnchor>, FactQueryError> {
        match symbol {
            BoundReferenceTarget::Surface(symbol) => Ok(self
                .symbol_graph()?
                .declaration_syntax_anchor(symbol)
                .and_then(|syntax| self.versioned_syntax(syntax))),
            BoundReferenceTarget::Local(symbol) => {
                let Some((_, bound)) = self.bound_unit_for_syntax(syntax, cancellation)? else {
                    return Ok(None);
                };

                Ok(bound
                    .value()
                    .local_symbols()
                    .syntax_anchor(symbol)
                    .map(|syntax| {
                        BoundSourceAnchor::new(
                            syntax,
                            bound.value().key().source().source_version(),
                        )
                    }))
            }
        }
    }

    fn versioned_syntax(&self, syntax: SyntaxAnchor) -> Option<BoundSourceAnchor> {
        let version = self.source(syntax.source_id())?.version();

        Some(BoundSourceAnchor::new(syntax, version))
    }
}

fn push_reference(
    references: &mut BTreeMap<BoundReferenceTarget, Vec<BoundSourceAnchor>>,
    target: BoundReferenceTarget,
    anchor: BoundSourceAnchor,
) {
    references.entry(target).or_default().push(anchor);
}

fn is_accessible_completion_symbol(
    graph: &bray_symbols::SymbolGraph,
    symbol: AnySymbolId,
    origin: Option<SymbolOrigin>,
) -> bool {
    matches!(origin, Some(SymbolOrigin::Source))
        || graph
            .symbol_visibility(symbol)
            .is_none_or(|visibility| visibility.is_public())
}

fn reference_sort_key(
    reference: &BoundSourceAnchor,
) -> (SourceId, TextSize, TextSize, bray_syntax::SyntaxKind) {
    let syntax = reference.syntax();

    (
        syntax.source_id(),
        syntax.full_range().start(),
        syntax.full_range().end(),
        syntax.syntax_kind(),
    )
}

fn declaration_for_syntax(
    declarations: &DeclarationTable,
    syntax: SyntaxAnchor,
) -> Option<&DeclarationRecord> {
    declarations
        .declarations()
        .iter()
        .filter(|declaration| syntax_contains(declaration.syntax_anchor(), syntax))
        .min_by(|left, right| {
            compare_syntax_ranges(left.syntax_anchor(), right.syntax_anchor())
                .then_with(|| left.id().cmp(&right.id()))
        })
}

fn declaration_at_syntax(
    declarations: &DeclarationTable,
    syntax: SyntaxAnchor,
) -> Option<&DeclarationRecord> {
    declarations
        .declarations()
        .iter()
        .find(|declaration| declaration.syntax_anchor() == syntax)
}

fn expression_for_syntax(bound: &BoundUnit, syntax: SyntaxAnchor) -> Option<BoundExpressionId> {
    bound
        .tree()
        .expressions()
        .filter_map(|(expression_id, expression)| {
            let origin = expression.origin();
            let anchor = origin.source_anchor().syntax();

            syntax_contains(anchor, syntax).then_some((
                expression_id,
                anchor,
                origin.synthesized_origin().is_some(),
            ))
        })
        .min_by(|left, right| {
            compare_syntax_ranges(left.1, right.1)
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.0.cmp(&right.0))
        })
        .map(|(expression, _, _)| expression)
}

fn compare_syntax_ranges(left: SyntaxAnchor, right: SyntaxAnchor) -> Ordering {
    left.full_range()
        .len()
        .cmp(&right.full_range().len())
        .then_with(|| left.full_range().start().cmp(&right.full_range().start()))
        .then_with(|| left.syntax_kind().cmp(&right.syntax_kind()))
}

fn syntax_contains(container: SyntaxAnchor, syntax: SyntaxAnchor) -> bool {
    container.source_id() == syntax.source_id()
        && container.full_range().contains_range(syntax.full_range())
}

fn range_covers_position(range: TextRange, position: TextSize) -> bool {
    range.contains(position) || (range.is_empty() && range.start() == position)
}

fn availability<T>(recovered: bool, value: T) -> SemanticAvailability<T> {
    if recovered {
        SemanticAvailability::Recovered(Some(value))
    } else {
        SemanticAvailability::Available(value)
    }
}

fn recovery_or_unavailable<T>(recovered: bool, value: Option<T>) -> SemanticAvailability<T> {
    match (recovered, value) {
        (true, value) => SemanticAvailability::Recovered(value),
        (false, Some(value)) => SemanticAvailability::Available(value),
        (false, None) => SemanticAvailability::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundReferenceTarget, BoundSourceAnchor, BoundUnitKind};
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceId, SourceVersion, TextSize};

    use super::SemanticAvailability;
    use crate::fact::{CancellationToken, FactCellTestEvent, QueryPriority};
    use crate::test_support::{
        FactEvaluationLog, FactTestGate, compilation, compilation_with_options,
        compilation_with_sources_and_worker_budget,
    };
    use crate::{CompilationOptions, SelectedTarget, SemanticAnalysisLimits, WorkerBudget};

    const SOURCE: &str = concat!(
        "module app;\n",
        "func identity(pos value: i32) -> i32\n",
        "{\n",
        "    let copy: i32 = value;\n",
        "    return copy;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let answer: i32 = identity(1);\n",
        "}\n",
    );

    #[test]
    fn source_unit_query_does_not_bind_units_from_other_sources() {
        let first = concat!("module app;\n", "func first()\n", "{\n", "}\n");
        let second = concat!("module app;\n", "func second()\n", "{\n", "}\n");

        let compilation =
            compilation_with_sources_and_worker_budget(&[first, second], WorkerBudget::serial());

        let units = compilation
            .bound_units_for_source(
                SourceId::new(0),
                &CancellationToken::new(),
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("source bound units must be available: {error:?}"));

        assert_eq!(units.len(), 1);

        assert!(
            units.iter().all(|unit| {
                unit.value().key().source().syntax().source_id() == SourceId::new(0)
            })
        );

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("unit keys must be available: {error:?}"));

        let second_source_keys = keys
            .iter()
            .filter(|key| key.source().syntax().source_id() == SourceId::new(1));

        assert!(second_source_keys.into_iter().all(|key| {
            compilation
                .state
                .bound_units
                .is_published(key)
                .is_ok_and(|published| !published)
        }));
    }

    #[test]
    fn position_queries_resolve_syntax_declarations_symbols_definitions_and_types() {
        let compilation = compilation(SOURCE);
        let cancellation = CancellationToken::new();
        let source_id = SourceId::new(0);
        let call_position = position(SOURCE, "identity(1)");

        let syntax = compilation
            .syntax_at(
                source_id,
                call_position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("syntax query must complete: {error:?}"))
            .unwrap_or_else(|| panic!("call syntax must be available"));

        let declaration = compilation
            .declaration_at(
                source_id,
                position(SOURCE, "func main"),
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("declaration query must complete: {error:?}"));

        assert!(matches!(declaration, SemanticAvailability::Available(_)));

        let symbol = compilation
            .symbol_for_syntax(syntax, &cancellation, QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("symbol query must complete: {error:?}"));

        assert!(matches!(
            symbol,
            SemanticAvailability::Available(BoundReferenceTarget::Surface(_))
        ));

        let references = compilation
            .references_at(
                source_id,
                call_position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("reference query must complete: {error:?}"));

        assert!(matches!(
            references,
            SemanticAvailability::Available(references) if references.len() == 2
        ));

        let first_index = compilation.state.source_reference_indexes[0]
            .get()
            .map(std::ptr::from_ref)
            .unwrap_or_else(|| panic!("source reference index must be published"));

        let _ = compilation
            .references_at(
                source_id,
                call_position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("repeated reference query must complete: {error:?}"));

        let repeated_index = compilation.state.source_reference_indexes[0]
            .get()
            .map(std::ptr::from_ref)
            .unwrap_or_else(|| panic!("source reference index must remain published"));

        assert_eq!(first_index, repeated_index);

        let definition = compilation
            .definition_for_syntax(syntax, &cancellation, QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("definition query must complete: {error:?}"));

        let SemanticAvailability::Available(definition) = definition else {
            panic!("source call must resolve a source definition");
        };

        assert!(
            definition
                .syntax()
                .full_range()
                .contains(position(SOURCE, "func identity"))
        );

        let expression_type = compilation
            .expression_type_for_syntax(syntax, &cancellation, QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("type query must complete: {error:?}"));

        assert!(matches!(
            expression_type,
            SemanticAvailability::Available(result) if !result.is_recovered()
        ));

        let local_position = position_after(SOURCE, "return ");

        let local_symbol = compilation
            .symbol_at(
                source_id,
                local_position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("local symbol query must complete: {error:?}"));

        assert!(matches!(
            local_symbol,
            SemanticAvailability::Available(BoundReferenceTarget::Local(_))
        ));

        let local_definition = compilation
            .definition_at(
                source_id,
                local_position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("local definition query must complete: {error:?}"));

        let SemanticAvailability::Available(local_definition) = local_definition else {
            panic!("local reference must resolve its parameter definition");
        };

        assert!(
            local_definition
                .syntax()
                .full_range()
                .contains(position(SOURCE, "copy: i32"))
        );
    }

    #[test]
    fn narrow_queries_do_not_bind_unrelated_bodies_or_request_package_diagnostics() {
        let compilation = compilation(SOURCE);
        let cancellation = CancellationToken::new();
        let call_position = position(SOURCE, "identity(1)");
        let evaluations = FactEvaluationLog::default();

        compilation
            .state
            .fact_runtime
            .set_test_observer(evaluations.observer())
            .unwrap_or_else(|error| panic!("runtime must accept test observation: {error:?}"));

        let symbol = compilation
            .symbol_at(
                SourceId::new(0),
                call_position,
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("symbol query must complete: {error:?}"));

        assert!(matches!(symbol, SemanticAvailability::Available(_)));
        assert!(compilation.state.check_diagnostics.get().is_none());

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("unit keys must be available: {error:?}"));

        let identity_position = position(SOURCE, "return copy");

        let identity = keys
            .iter()
            .find(|key| {
                key.source()
                    .syntax()
                    .full_range()
                    .contains(identity_position)
            })
            .unwrap_or_else(|| panic!("identity body key must be available"));

        let main = keys
            .iter()
            .find(|key| key.source().syntax().full_range().contains(call_position))
            .unwrap_or_else(|| panic!("main body key must be available"));

        assert_eq!(
            compilation.state.bound_units.is_published(identity),
            Ok(false)
        );

        assert_eq!(compilation.state.bound_units.is_published(main), Ok(true));

        assert_eq!(
            compilation.state.expression_semantics.is_published(main),
            Ok(false)
        );

        let evaluated = evaluations.keys();

        assert!(evaluated.contains(&crate::fact::CompilationFactKey::BoundUnit(main.clone())));

        assert!(
            !evaluated.contains(&crate::fact::CompilationFactKey::BoundUnit(
                identity.clone()
            ))
        );

        assert!(!evaluated.contains(&crate::fact::CompilationFactKey::CheckDiagnostics));

        assert!(
            !evaluated.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
                main.clone()
            ))
        );
    }

    #[test]
    fn cancelled_queries_return_without_publishing_semantic_bodies() {
        let compilation = compilation(SOURCE);
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let result = compilation.symbol_at(
            SourceId::new(0),
            position(SOURCE, "identity(1)"),
            &cancellation,
            QueryPriority::Interactive,
        );

        assert!(matches!(result, Err(crate::FactQueryError::Cancelled)));

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("unit keys must be available: {error:?}"));

        assert!(keys.iter().all(|key| {
            compilation
                .state
                .bound_units
                .is_published(key)
                .is_ok_and(|published| !published)
        }));
    }

    #[test]
    fn syntax_identity_queries_reject_other_source_revisions() {
        let compilation = compilation(SOURCE);
        let cancellation = CancellationToken::new();

        let syntax = compilation
            .syntax_at(
                SourceId::new(0),
                position(SOURCE, "identity(1)"),
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("syntax query must complete: {error:?}"))
            .unwrap_or_else(|| panic!("call syntax must be available"));

        let stale = BoundSourceAnchor::new(
            syntax.syntax(),
            SourceVersion::new(syntax.source_version().raw() + 1),
        );

        let symbol = compilation
            .symbol_for_syntax(stale, &cancellation, QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("stale symbol query must complete: {error:?}"));

        assert_eq!(symbol, SemanticAvailability::Unavailable);
    }

    #[test]
    fn symbol_queries_do_not_use_enclosing_declarations_as_fallbacks() {
        let compilation = compilation(SOURCE);
        let cancellation = CancellationToken::new();

        let symbol = compilation
            .symbol_at(
                SourceId::new(0),
                position(SOURCE, "i32 = identity"),
                &cancellation,
                QueryPriority::Interactive,
            )
            .unwrap_or_else(|error| panic!("type syntax symbol query must complete: {error:?}"));

        assert_eq!(symbol, SemanticAvailability::Unavailable);
    }

    #[test]
    fn diagnostic_queries_preserve_source_unit_and_package_scope() {
        let compilation = compilation(concat!(
            "module app\n",
            "func main()\n",
            "{\n",
            "    missing;\n",
            "}\n",
        ));

        let cancellation = CancellationToken::new();

        let source_diagnostics = compilation
            .diagnostics_for_source(SourceId::new(0), &cancellation, QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("source diagnostics must complete: {error:?}"));

        assert!(
            source_diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.kind() == DiagnosticKind::SyntaxExpectedToken })
        );

        assert!(compilation.state.check_diagnostics.get().is_none());

        let key = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("unit keys must be available: {error:?}"))
            .iter()
            .find(|key| key.kind() == BoundUnitKind::CallableBody)
            .cloned()
            .unwrap_or_else(|| panic!("callable body key must be available"));

        let unit_diagnostics = compilation
            .diagnostics_for_unit(key, &cancellation, QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("unit diagnostics must complete: {error:?}"));

        assert!(
            unit_diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.kind() == DiagnosticKind::BindingUnresolvedName })
        );

        assert!(compilation.state.check_diagnostics.get().is_none());

        let package_diagnostics = compilation
            .diagnostics_for_package(&cancellation, QueryPriority::Background)
            .unwrap_or_else(|error| panic!("package diagnostics must complete: {error:?}"));

        assert!(
            package_diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.kind() == DiagnosticKind::BindingUnresolvedName })
        );

        assert!(compilation.state.check_diagnostics.get().is_some());
    }

    #[test]
    fn resource_exhaustion_is_deterministic_across_workers_priorities_and_shared_waiters() {
        const SOURCE: &str = concat!(
            "module app;\n",
            "\n",
            "func first(pos value: bool)\n",
            "{\n",
            "}\n",
            "\n",
            "func second(pos value: bool)\n",
            "{\n",
            "}\n",
            "\n",
            "overload choose =\n",
            "{\n",
            "    first,\n",
            "    second,\n",
            "}\n",
        );

        let limits = SemanticAnalysisLimits::new(256, 0);

        let serial = compilation_with_options(
            SOURCE,
            CompilationOptions::new(
                WorkerBudget::serial(),
                bray_symbols::ProductKind::Library,
                SelectedTarget::baseline(),
            )
            .with_semantic_analysis_limits(limits),
        );

        let serial_diagnostics = serial
            .diagnostics_for_package(&CancellationToken::new(), QueryPriority::Interactive)
            .unwrap_or_else(|error| panic!("serial limited query must complete: {error:?}"));

        let parallel_budget = WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("parallel worker budget must be valid: {error:?}"));

        let parallel = compilation_with_options(
            SOURCE,
            CompilationOptions::new(
                parallel_budget,
                bray_symbols::ProductKind::Library,
                SelectedTarget::baseline(),
            )
            .with_semantic_analysis_limits(limits),
        );

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        parallel
            .state
            .check_diagnostics
            .set_test_observer(gate.observer())
            .unwrap_or_else(|error| panic!("diagnostic fact must accept observation: {error:?}"));

        let parallel_diagnostics = std::thread::scope(|scope| {
            let parallel = &parallel;
            let background_cancellation = CancellationToken::new();
            let interactive_cancellation = CancellationToken::new();

            let background = scope.spawn(move || {
                parallel
                    .diagnostics_for_package(&background_cancellation, QueryPriority::Background)
            });

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let interactive = scope.spawn(move || {
                parallel
                    .diagnostics_for_package(&interactive_cancellation, QueryPriority::Interactive)
            });

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            [background, interactive].map(|request| {
                request
                    .join()
                    .unwrap_or_else(|_| panic!("limited query must not panic"))
                    .unwrap_or_else(|error| panic!("limited query must complete: {error:?}"))
            })
        });

        assert_eq!(parallel_diagnostics[0], serial_diagnostics);
        assert_eq!(parallel_diagnostics[1], serial_diagnostics);

        assert!(serial_diagnostics.iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingCallableOverloadLimitExceeded
        }));

        let repeated = parallel
            .diagnostics_for_package(&CancellationToken::new(), QueryPriority::Normal)
            .unwrap_or_else(|error| panic!("repeated limited query must complete: {error:?}"));

        assert_eq!(repeated, serial_diagnostics);
    }

    #[test]
    fn cancelled_package_diagnostics_publish_no_partial_result() {
        let compilation = compilation(SOURCE);
        let cancellation = CancellationToken::new();
        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        compilation
            .state
            .check_diagnostics
            .set_test_observer(gate.observer())
            .unwrap_or_else(|error| panic!("diagnostic fact must accept an observer: {error:?}"));

        let result = std::thread::scope(|scope| {
            let request = scope.spawn(|| {
                compilation.diagnostics_for_package(&cancellation, QueryPriority::Interactive)
            });

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);
            cancellation.cancel();
            gate.release();

            request
                .join()
                .unwrap_or_else(|_| panic!("cancelled diagnostic query must not panic"))
        });

        assert!(matches!(result, Err(crate::FactQueryError::Cancelled)));
        assert!(compilation.state.check_diagnostics.get().is_none());
    }

    fn position(source: &str, text: &str) -> TextSize {
        let offset = source
            .find(text)
            .unwrap_or_else(|| panic!("test source must contain {text:?}"));

        TextSize::try_from(offset)
            .unwrap_or_else(|_| panic!("test source offset must fit in TextSize"))
    }

    fn position_after(source: &str, text: &str) -> TextSize {
        let position = position(source, text);

        let length = TextSize::try_from(text.len())
            .unwrap_or_else(|_| panic!("test text length must fit in TextSize"));

        position
            .checked_add(length)
            .unwrap_or_else(|| panic!("test source position must fit in TextSize"))
    }
}
