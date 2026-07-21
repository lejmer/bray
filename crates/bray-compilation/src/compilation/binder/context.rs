use std::sync::Arc;

use bray_binder::{
    BinderFactContext, BinderFactError, BinderFactResult, ImportedPathRoot, TargetFactProvider,
    TargetFactResult,
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

    pub(super) fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        let mut package_prefix = String::new();
        let mut selected = None;

        for component in components {
            if !package_prefix.is_empty() {
                package_prefix.push('.');
            }

            package_prefix.push_str(component);

            let dependency = self
                .compilation
                .state
                .dependency_interfaces
                .binary_search_by(|dependency| {
                    dependency.package().as_str().cmp(package_prefix.as_str())
                })
                .ok();

            if let Some(dependency) = dependency {
                selected = Some(dependency);
            }
        }

        let Some(dependency) = selected else {
            return Ok(None);
        };

        let identity = self.compilation.state.dependency_interfaces[dependency].package();

        let symbols = self
            .imported_symbols()?
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let package = symbols
            .package_by_identity(identity)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        ImportedPathRoot::for_path(symbols, package.id(), components)
            .map(Some)
            .ok_or(BinderFactError::DependencyUnavailable)
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

    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        CompilationBinderFacts::imported_path_root(self, components)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::BinderFactError;
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationPolicy,
        test_support::encoded_template_test_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;

    use super::CompilationBinderFacts;
    use crate::{CancellationToken, Compilation, CompilationRequest, DependencyInterfaceInput};

    #[test]
    fn unrelated_paths_do_not_demand_dependency_interfaces() {
        let fixture = encoded_template_test_interface();
        let compilation = compilation([valid_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        assert!(matches!(
            CompilationBinderFacts::imported_path_root(&facts, &["local", "value"]),
            Ok(None)
        ));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        assert!(
            compilation
                .state
                .loaded_dependency_interfaces
                .iter()
                .all(|interface| interface.get().is_none())
        );
    }

    #[test]
    fn matching_paths_demand_only_the_imported_identity_skeleton() {
        let fixture = encoded_template_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([valid_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        let root = CompilationBinderFacts::imported_path_root(&facts, &components)
            .unwrap_or_else(|error| panic!("imported path root must be available: {error:?}"));

        assert!(root.is_some());
        assert!(compilation.state.imported_symbol_skeleton.get().is_some());

        assert!(
            compilation
                .state
                .imported_semantic_graphs
                .iter()
                .all(|graph| graph.get().is_none())
        );

        assert_eq!(
            root.map(|root| root.consumed_components()),
            Some(components.len())
        );
    }

    #[test]
    fn cancelled_imported_path_lookup_does_not_publish_a_skeleton() {
        let fixture = encoded_template_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([valid_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        cancellation.cancel();

        assert!(matches!(
            CompilationBinderFacts::imported_path_root(&facts, &components),
            Err(BinderFactError::Cancelled)
        ));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());
    }

    #[test]
    fn selected_invalid_dependencies_are_unavailable_instead_of_absent() {
        let dependency = DependencyInterfaceInput::new(
            package("invalid.package"),
            bray_package_interface::InterfaceProductIdentity::try_new("main")
                .unwrap_or_else(|| panic!("test product identity must be valid")),
            "invalid.brayi",
            Arc::<[u8]>::from(vec![0; 112]),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let compilation = compilation([dependency]);
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        assert!(matches!(
            CompilationBinderFacts::imported_path_root(&facts, &["invalid", "package"]),
            Err(BinderFactError::DependencyUnavailable)
        ));
    }

    fn valid_dependency(
        fixture: &bray_package_interface::test_support::EncodedTemplateTestInterface,
    ) -> DependencyInterfaceInput {
        DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes.clone()),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
    }

    fn compilation(
        dependencies: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Compilation {
        let request = CompilationRequest::new(
            package("current.package"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(1),
                "main.bray",
                SourceVersion::new(1),
                "module current.package;",
            )],
        )
        .with_dependency_interfaces(dependencies);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }
}
