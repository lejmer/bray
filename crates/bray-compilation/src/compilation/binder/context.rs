use std::sync::Arc;

use bray_binder::{
    BinderFactContext, BinderFactError, BinderFactResult, TargetFactProvider, TargetFactResult,
};
use bray_declarations::DeclarationTable;
use bray_symbols::{
    AnySymbolId, ConstantSymbolId, ImportedSymbolFactAddress, ImportedSymbolSkeleton,
    SemanticValueStore, SymbolGraph,
};
use bray_syntax::SyntaxTree;

use super::super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

pub(in crate::compilation) struct CompilationBinderFacts<'compilation> {
    pub(super) compilation: &'compilation Compilation,
    pub(super) symbols: &'compilation SymbolGraph,
    pub(super) semantic_values: &'compilation SemanticValueStore,
    pub(super) cancellation: &'compilation CancellationToken,
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

    pub(in crate::compilation) const fn compilation(&self) -> &'compilation Compilation {
        self.compilation
    }

    pub(super) fn imported_symbols_for_package(
        &self,
        package: &str,
    ) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        if !self
            .compilation
            .state
            .dependency_interfaces
            .iter()
            .any(|dependency| dependency.package().as_str() == package)
        {
            return Ok(None);
        }

        self.imported_symbols()
    }

    pub(super) fn imported_symbols(&self) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        let result = self
            .compilation
            .imported_symbol_skeleton_result_with_cancellation(self.cancellation)
            .map_err(|error| match error {
                FactQueryError::Cancelled => BinderFactError::Cancelled,
                _ => BinderFactError::DependencyUnavailable,
            })?;

        Ok(result.value().as_deref())
    }

    pub(super) fn imported_fact_address(
        &self,
        symbol: AnySymbolId,
    ) -> BinderFactResult<Option<ImportedSymbolFactAddress>> {
        if self.symbols.symbol_key(symbol).is_some() {
            return Ok(None);
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.imported_fact_address(symbol)))
    }
}

impl BinderFactContext for CompilationBinderFacts<'_> {
    type TargetFacts = CompilationTargetFacts;
    type SymbolFacts = Self;
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

    fn imported_symbols_for_package(
        &self,
        package: &str,
    ) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        CompilationBinderFacts::imported_symbols_for_package(self, package)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn target_facts(&self) -> &Self::TargetFacts {
        &self.compilation.state.target_facts
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        self
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

#[derive(Debug)]
pub(in crate::compilation) struct CompilationTargetFacts;

impl TargetFactProvider for CompilationTargetFacts {
    fn target_fact(&self, _fact: ConstantSymbolId) -> BinderFactResult<Arc<TargetFactResult>> {
        Err(BinderFactError::DependencyUnavailable)
    }
}

impl Compilation {
    pub(in crate::compilation) fn binder_facts_for<'compilation>(
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

    pub(in crate::compilation) fn binder_facts<'compilation>(
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
