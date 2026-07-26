use std::cmp::Ordering;
use std::sync::Arc;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundSourceAnchor, BoundUnit,
    BoundUnitKey, ExpressionTypeResult, SemanticSelection,
};
use bray_declarations::{DeclarationId, DeclarationRecord, DeclarationTable, SyntaxAnchor};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::{SourceId, TextRange, TextSize};
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_node};

use super::facts::Compilation;
use crate::fact::{CancellationToken, FactQueryError, QueryPriority};

type BoundUnitFact = Arc<DiagnosticResult<BoundUnit>>;
type SyntaxBoundExpression = (BoundUnitFact, BoundExpressionId);
type SyntaxBoundUnit = (BoundUnitKey, BoundUnitFact);

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
    use crate::test_support::{FactTestGate, compilation};

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
