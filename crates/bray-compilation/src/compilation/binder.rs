use std::sync::Arc;

use bray_binder::{
    BinderFactContext, BinderFactError, BinderFactResult, TargetFactProvider, TargetFactResult,
};
use bray_declarations::DeclarationTable;
use bray_symbols::{ConstantSymbolId, SemanticValueStore, SymbolGraph};
use bray_syntax::SyntaxTree;

use super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

pub(super) struct CompilationBinderFacts<'compilation> {
    compilation: &'compilation Compilation,
    symbols: &'compilation SymbolGraph,
    semantic_values: &'compilation SemanticValueStore,
    cancellation: &'compilation CancellationToken,
}

impl<'compilation> CompilationBinderFacts<'compilation> {
    pub(super) const fn new(
        compilation: &'compilation Compilation,
        symbols: &'compilation SymbolGraph,
        semantic_values: &'compilation SemanticValueStore,
        cancellation: &'compilation CancellationToken,
    ) -> Self {
        Self {
            compilation,
            symbols,
            semantic_values,
            cancellation,
        }
    }
}

impl BinderFactContext for CompilationBinderFacts<'_> {
    type TargetFacts = CompilationTargetFacts;
    type SymbolFacts = ();
    type Cancellation = CancellationToken;

    fn syntax(&self) -> &SyntaxTree {
        self.compilation.syntax_tree()
    }

    fn declarations(&self) -> &DeclarationTable {
        self.compilation.declaration_table()
    }

    fn symbols(&self) -> &SymbolGraph {
        self.symbols
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn target_facts(&self) -> &Self::TargetFacts {
        &self.compilation.state.target_facts
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        &self.compilation.state.symbol_facts
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

#[derive(Debug)]
pub(super) struct CompilationTargetFacts;

impl TargetFactProvider for CompilationTargetFacts {
    fn target_fact(&self, _fact: ConstantSymbolId) -> BinderFactResult<Arc<TargetFactResult>> {
        Err(BinderFactError::DependencyUnavailable)
    }
}

impl Compilation {
    pub(super) fn binder_facts_for<'compilation>(
        &'compilation self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBinderFacts<'compilation>, FactQueryError> {
        let facts = self.binder_facts(cancellation)?;

        if !facts.symbols.contains_symbol_key(key.declared_owner()) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        Ok(facts)
    }

    pub(super) fn binder_facts<'compilation>(
        &'compilation self,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBinderFacts<'compilation>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let semantic_values = self.semantic_value_store()?;

        Ok(CompilationBinderFacts::new(
            self,
            symbols,
            semantic_values,
            cancellation,
        ))
    }
}
