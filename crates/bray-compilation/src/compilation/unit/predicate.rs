use crate::compilation::Compilation;
use crate::fact::CompilationFactKey;
use bray_bound_tree::BoundUnitKey;
use bray_symbols::PredicateDefinitionSymbolId;
use std::collections::BTreeMap;

impl Compilation {
    pub(in crate::compilation) fn predicate_definition_key(
        &self,
        owner: PredicateDefinitionSymbolId,
    ) -> Result<Option<BoundUnitKey>, crate::fact::FactQueryError> {
        let result = self.evaluate_query(
            CompilationFactKey::PredicateDefinitionKeys,
            &self.state.predicate_definition_keys,
            || {
                let symbols = self.symbol_graph()?;
                let mut definitions = BTreeMap::new();

                for key in self.declared_unit_keys()? {
                    if key.kind() != bray_bound_tree::BoundUnitKind::PredicateDefinition {
                        continue;
                    }

                    let symbol = symbols
                        .symbol_for_key(key.declared_owner())
                        .ok_or_else(|| {
                            crate::compilation::SemanticQueryFailure::contract(
                                crate::compilation::SemanticQueryContext::Unit(key.clone()),
                                crate::compilation::SemanticQueryViolation::Missing(
                                    crate::compilation::SemanticDataKind::Symbol,
                                ),
                            )
                        })?;

                    let symbol =
                        PredicateDefinitionSymbolId::try_from_any(symbol).ok_or_else(|| {
                            crate::compilation::SemanticQueryFailure::contract(
                            crate::compilation::SemanticQueryContext::Symbol(symbol),
                            crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                                expected:
                                    crate::compilation::SemanticSymbolCategory::PredicateDefinition,
                                actual: symbol.kind(),
                            },
                        )
                        })?;

                    if definitions.insert(symbol, key).is_some() {
                        return Err(crate::compilation::SemanticQueryFailure::contract(
                            crate::compilation::SemanticQueryContext::Symbol(symbol.into_any()),
                            crate::compilation::SemanticQueryViolation::CountMismatch {
                                data: crate::compilation::SemanticDataKind::BoundUnit,
                                expected: 1,
                                actual: 2,
                            },
                        )
                        .into());
                    }
                }

                Ok(definitions)
            },
        );

        // Query results retain shared keys and failures beyond the cached map borrow.
        match result {
            Ok(definitions) => Ok(definitions.get(&owner).cloned()),
            Err(error) => Err(error.clone()),
        }
    }
}
