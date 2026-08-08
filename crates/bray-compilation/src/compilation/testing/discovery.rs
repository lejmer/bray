use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::lowercase_hex;
use bray_diagnostics::DiagnosticResult;
use bray_source::SourceSpan;
use bray_symbols::{FunctionSymbolId, ProductIdentity, ProductKind};
use bray_test_protocol::{
    TestCatalog, TestDeclarationPath, TestEntryMetadata, TestErrorTypeIdentity, TestIdentity,
    TestSourceAnchor,
};

use crate::compilation::Compilation;
use crate::compilation::product::structural_type_identity;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

/// Stable test metadata paired with compilation-local callable identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestDiscovery {
    catalog: TestCatalog,
    functions: BTreeMap<TestIdentity, FunctionSymbolId>,
}

impl TestDiscovery {
    fn new(
        catalog: TestCatalog,
        functions: impl IntoIterator<Item = (TestIdentity, FunctionSymbolId)>,
    ) -> Self {
        Self {
            catalog,
            functions: functions.into_iter().collect(),
        }
    }

    /// Returns the metadata-only catalog in canonical test identity order.
    pub const fn catalog(&self) -> &TestCatalog {
        &self.catalog
    }

    /// Returns the compilation-local function for an exact catalog entry.
    pub fn function(&self, identity: &TestIdentity) -> Option<FunctionSymbolId> {
        self.functions.get(identity).copied()
    }
}

impl Compilation {
    /// Returns canonical test discovery metadata for one selected package product.
    pub fn test_discovery(
        &self,
        product: ProductIdentity,
    ) -> Result<Arc<DiagnosticResult<TestDiscovery>>, FactQueryError> {
        self.test_discovery_with_cancellation(product, &self.state.cancellation)
    }

    pub(in crate::compilation) fn test_discovery_with_cancellation(
        &self,
        product: ProductIdentity,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<TestDiscovery>>, FactQueryError> {
        let cell = self.state.test_discoveries.cell(product.clone())?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::TestDiscovery(product.clone()),
            cancellation,
            || {
                self.compute_test_discovery(product, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_test_discovery(
        &self,
        product: ProductIdentity,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<TestDiscovery>, FactQueryError> {
        if product.package() != self.package_identity()
            && self.options().product_kind() != ProductKind::Test
        {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let semantic = self.product_semantic_facts_with_cancellation(cancellation)?;
        let symbols = self.symbol_graph()?;
        let binder = self.binder_facts(cancellation)?;
        let semantic_values = self.semantic_value_store()?;
        let mut entries = Vec::new();

        if semantic.value().kind() == ProductKind::Test {
            for test in semantic.value().test_entries() {
                cancellation.check()?;

                let anchor = symbols
                    .declaration_syntax_anchor(test.function().into())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let source = self
                    .source(anchor.source_id())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let declaration =
                    TestDeclarationPath::new(test.module().clone(), test.name().clone());

                let identity = TestIdentity::new(product.clone(), declaration);

                let source = TestSourceAnchor::new(
                    SourceSpan::new(anchor.source_id(), anchor.full_range()),
                    source.version(),
                );

                let error_type = test
                    .error_type()
                    .map(|error| structural_type_identity(semantic_values, &binder, error))
                    .transpose()?
                    .map(error_type_identity)
                    .transpose()?;

                entries.push((
                    TestEntryMetadata::new(
                        identity,
                        source,
                        test.execution(),
                        test.constraint(),
                        test.result(),
                        error_type,
                    ),
                    test.function(),
                ));
            }
        }

        let catalog = TestCatalog::try_new(product, entries.iter().map(|(entry, _)| entry.clone()))
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let discovery = TestDiscovery::new(
            catalog,
            entries
                .into_iter()
                .map(|(entry, function)| (entry.identity().clone(), function)),
        );

        Ok(DiagnosticResult::new(
            discovery,
            semantic.diagnostics().clone(),
        ))
    }
}

fn error_type_identity(digest: [u8; 32]) -> Result<TestErrorTypeIdentity, FactQueryError> {
    TestErrorTypeIdentity::try_new(lowercase_hex(&digest))
        .ok_or(FactQueryError::InfrastructureFailure)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::Arc;

    use bray_symbols::{
        CallableExecution, ProductIdentity, ProductKind, TestExecutionConstraint, TestResultShape,
    };
    use bray_test_protocol::{TestFilter, TestSelection, TestSelectionQuery, TestShard};

    use crate::WorkerBudget;
    use crate::test_support::{
        compilation_with_sources_product_and_worker_budget, package_identity,
    };

    #[test]
    fn discovery_publishes_cached_canonical_test_metadata() {
        let compilation = compilation_with_sources_product_and_worker_budget(
            &[
                "module z.tests;\n\n@test(serial)\nasync func slower()\n{\n}\n",
                "module a.tests;\n\n@test\nfunc faster()\n{\n}\n",
            ],
            ProductKind::Test,
            parallel_worker_budget(),
        );

        let product = product();
        let first = discovery(&compilation, product.clone());
        let second = discovery(&compilation, product);
        let entries = first.value().catalog().entries();

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.identity().declaration().name().as_str())
                .collect::<Vec<_>>(),
            ["faster", "slower"]
        );

        assert_eq!(
            entries[0].identity().declaration().name().as_str(),
            "faster"
        );

        assert_eq!(entries[0].execution(), CallableExecution::Synchronous);
        assert_eq!(entries[0].constraint(), TestExecutionConstraint::Parallel);
        assert_eq!(entries[0].result(), TestResultShape::Unit);

        assert_eq!(
            entries[1].identity().declaration().name().as_str(),
            "slower"
        );

        assert_eq!(entries[1].execution(), CallableExecution::Asynchronous);
        assert_eq!(entries[1].constraint(), TestExecutionConstraint::Serial);
        assert!(first.value().function(entries[0].identity()).is_some());
    }

    #[test]
    fn catalog_filters_and_shards_without_source_access() {
        let compilation = compilation_with_sources_product_and_worker_budget(
            &[
                "module z.tests;\n\n@test(serial)\nfunc gamma()\n{\n}\n",
                "module a.tests;\n\n@test\nfunc alpha()\n{\n}\n\n@test\nfunc beta()\n{\n}\n",
            ],
            ProductKind::Test,
            parallel_worker_budget(),
        );

        let discovery = discovery(&compilation, product());
        let catalog = discovery.value().catalog();
        let alpha = &catalog.entries()[0];

        let name = TestFilter::name_contains("a")
            .unwrap_or_else(|| panic!("non-empty test name filter must be valid"));

        let serial = TestSelection::from_catalog(
            catalog,
            &TestSelectionQuery::new(
                [
                    name,
                    TestFilter::Constraint(TestExecutionConstraint::Serial),
                ],
                None,
            ),
        );

        assert_eq!(serial.entries().len(), 1);

        assert_eq!(
            serial.entries()[0].identity().declaration().name().as_str(),
            "gamma"
        );

        let exact_filters = [
            TestFilter::Package(catalog.product().package().clone()),
            TestFilter::Product(catalog.product().clone()),
            TestFilter::Module(alpha.identity().declaration().module().clone()),
            TestFilter::Declaration(alpha.identity().declaration().clone()),
            TestFilter::Identity(alpha.identity().clone()),
        ];

        for filter in exact_filters {
            let selected =
                TestSelection::from_catalog(catalog, &TestSelectionQuery::new([filter], None));

            assert!(!selected.entries().is_empty());
        }

        assert_eq!(catalog.entry(alpha.identity()), Some(alpha));

        let shard_count =
            NonZeroU32::new(2).unwrap_or_else(|| panic!("test shard count must be nonzero"));

        let first_shard = TestShard::try_new(0, shard_count)
            .unwrap_or_else(|| panic!("first test shard must be valid"));

        let second_shard = TestShard::try_new(1, shard_count)
            .unwrap_or_else(|| panic!("second test shard must be valid"));

        let first =
            TestSelection::from_catalog(catalog, &TestSelectionQuery::new([], Some(first_shard)));

        let second =
            TestSelection::from_catalog(catalog, &TestSelectionQuery::new([], Some(second_shard)));

        assert_eq!(first.entries().len() + second.entries().len(), 3);
        assert_eq!(first.discovered_count(), 3);
        assert_eq!(second.discovered_count(), 3);
    }

    #[test]
    fn test_catalog_identity_can_differ_from_the_source_package() {
        let compilation = compilation_with_sources_product_and_worker_budget(
            &["module tests;\n\n@test\nfunc works()\n{\n}\n"],
            ProductKind::Test,
            parallel_worker_budget(),
        );

        let package = bray_symbols::PackageIdentity::try_new("public.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "tests")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let discovery = discovery(&compilation, product.clone());

        assert_eq!(discovery.value().catalog().product(), &product);

        assert_eq!(
            discovery.value().catalog().entries()[0]
                .identity()
                .product(),
            &product,
        );
    }

    fn discovery(
        compilation: &crate::Compilation,
        product: ProductIdentity,
    ) -> Arc<bray_diagnostics::DiagnosticResult<super::TestDiscovery>> {
        compilation
            .test_discovery(product)
            .unwrap_or_else(|error| panic!("test discovery must succeed: {error:?}"))
    }

    fn product() -> ProductIdentity {
        ProductIdentity::try_new(package_identity(), "tests")
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }

    fn parallel_worker_budget() -> WorkerBudget {
        WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("parallel test worker budget must be valid: {error:?}"))
    }
}
