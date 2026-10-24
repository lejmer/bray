use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bray_base::lowercase_hex;
use bray_compilation::{Compilation, CompilationOptions, SelectedTarget, WorkerBudget};
use bray_declarations::{DeclarationKind, DeclarationName, SyntaxAnchor};
use bray_project::ProjectProduct;
use bray_source::SourceSpan;
use bray_symbols::{
    CallableExecution, ModulePathKey, ProductIdentity, SymbolName, TestExecutionConstraint,
};
use bray_syntax::{SyntaxKind, TestDirectiveSyntax};
use bray_target::NativeTarget;
use bray_test_protocol::{
    TestCatalogDigest, TestDeclarationPath, TestIdentity, TestSourceAnchor, decode_test_catalog,
};

use super::command::BuildError;
use crate::native_test_report::{NativeOutcome, NativeTestReport};

/// Source declarations captured independently of checked test-entry discovery.
pub(super) struct SourceTestInventory {
    product: ProductIdentity,
    tests: BTreeMap<TestIdentity, SourceTest>,
    excluded: Vec<(TestIdentity, SourceSpan)>,
}

struct SourceTest {
    source: TestSourceAnchor,
    execution: CallableExecution,
    constraint: TestExecutionConstraint,
}

/// A published catalog whose identities and source metadata match source facts.
pub(super) struct VerifiedTestCatalog {
    product: ProductIdentity,
    identities: Vec<String>,
    digest: TestCatalogDigest,
}

impl SourceTestInventory {
    pub(super) fn load(
        workspace: &Path,
        product: &ProjectProduct,
        target: NativeTarget,
    ) -> Result<Self, BuildError> {
        if !product.targets().contains(&target.identity()) {
            return Err(failure(format!(
                "{} is unavailable for {}",
                product.identity().name(),
                target.as_str(),
            )));
        }

        let request = bray_tooling::compilation_request_from_file_arguments(
            product.identity().package().clone(),
            product.sources().iter().map(|path| path.beneath(workspace)),
            CompilationOptions::new(
                WorkerBudget::default(),
                product.kind(),
                SelectedTarget::for_native(target),
            ),
        )
        .map_err(|error| failure(format!("could not load product sources: {error:?}")))?
        .with_standard_library_source_authority();

        let compilation = Compilation::load(request)
            .map_err(|error| failure(format!("could not load source facts: {error:?}")))?;

        let inventory = Self::from_compilation(&compilation, product.identity().clone())?;

        println!(
            "{} {}: {} source tests, {} excluded by module availability",
            product.identity().name(),
            target.as_str(),
            inventory.tests.len(),
            inventory.excluded.len(),
        );

        for (identity, source) in &inventory.excluded {
            println!(
                "  excluded source declaration {identity} at {source:?} on {}",
                target.as_str()
            );
        }

        Ok(inventory)
    }

    fn from_compilation(
        compilation: &Compilation,
        product: ProductIdentity,
    ) -> Result<Self, BuildError> {
        let table = compilation.declaration_table();

        if compilation.declaration_diagnostics().has_errors() {
            return Err(failure(format!(
                "invalid source declarations: {:?}",
                compilation.declaration_diagnostics(),
            )));
        }

        let mut tests = BTreeMap::new();
        let mut excluded = Vec::new();

        // The unselected declaration table retains tests even if checked product
        // semantics or catalog emission later omits an entire source module.
        for part in table.module_parts() {
            let declarations = part
                .declarations()
                .iter()
                .filter_map(|id| {
                    let declaration = table
                        .declaration(*id)
                        .expect("module declaration ID must resolve");

                    (declaration.kind() == DeclarationKind::Function).then_some(declaration)
                })
                .filter_map(|declaration| {
                    declaration
                        .surface()
                        .directives()
                        .iter()
                        .find(|directive| directive.syntax_kind() == SyntaxKind::TestDirective)
                        .map(|directive| (declaration, *directive))
                })
                .collect::<Vec<_>>();

            if declarations.is_empty() {
                continue;
            }

            let gate = compilation
                .module_contribution_gate(part.id())
                .map_err(|error| {
                    failure(format!("could not evaluate module availability: {error:?}"))
                })?;

            if gate.diagnostics().has_errors() {
                return Err(failure(format!(
                    "invalid module availability: {:?}",
                    gate.diagnostics()
                )));
            }

            let module = table
                .container(part.module_container())
                .and_then(|container| container.module_path())
                .expect("module contribution must have a module path");

            let module = ModulePathKey::try_new(module.segments().iter().map(String::as_str))
                .expect("valid source module path must be nonempty");

            for (declaration, directive) in declarations {
                let name = declaration
                    .name()
                    .and_then(DeclarationName::as_identifier)
                    .and_then(SymbolName::try_new)
                    .expect("valid test declaration must have a name");

                let identity = TestIdentity::new(
                    product.clone(),
                    TestDeclarationPath::new(module.clone(), name),
                );

                let anchor = declaration.syntax_anchor();

                if !gate.value().is_enabled() {
                    excluded.push((
                        identity,
                        SourceSpan::new(anchor.source_id(), anchor.full_range()),
                    ));

                    continue;
                }

                let source = compilation
                    .source(anchor.source_id())
                    .expect("declaring source must exist");

                let test = SourceTest {
                    source: TestSourceAnchor::new(
                        product.source_namespace(),
                        SourceSpan::new(anchor.source_id(), anchor.full_range()),
                        source.version(),
                    ),
                    execution: if declaration
                        .surface()
                        .modifiers()
                        .contains(&SyntaxKind::AsyncKeyword)
                    {
                        CallableExecution::Asynchronous
                    } else {
                        CallableExecution::Synchronous
                    },
                    constraint: source_test_constraint(compilation, directive),
                };

                if let Some(previous) = tests.insert(identity.clone(), test) {
                    return Err(failure(format!(
                        "duplicate source test {identity}, first declared at {:?}, repeated at {:?}",
                        previous.source, anchor
                    )));
                }
            }
        }

        if tests.is_empty() {
            return Err(failure(format!(
                "{} has no applicable declared tests on {}, excluded {:?}",
                product,
                compilation
                    .selected_target()
                    .target()
                    .native_target()
                    .expect("test inventory selects a native target")
                    .as_str(),
                excluded
                    .iter()
                    .map(|(identity, source)| format!("{identity} at {source:?}"))
                    .collect::<Vec<_>>()
            )));
        }

        Ok(Self {
            product,
            tests,
            excluded,
        })
    }

    pub(super) fn verify(self, bytes: &[u8]) -> Result<VerifiedTestCatalog, BuildError> {
        let (catalog, digest) = decode_test_catalog(bytes).map_err(|error| match error {
            bray_test_protocol::TestCatalogDecodeError::Entries(
                bray_test_protocol::TestCatalogBuildError::DuplicateIdentity(identity),
            ) => failure(format!("duplicate published test {identity}")),
            error => failure(format!("could not decode published catalog: {error:?}")),
        })?;

        if catalog.product() != &self.product {
            return Err(failure(format!(
                "published catalog belongs to {:?}, expected {:?}",
                catalog.product(),
                self.product
            )));
        }

        let actual = catalog
            .entries()
            .iter()
            .map(|entry| entry.identity())
            .collect::<BTreeSet<_>>();

        let expected = self.tests.keys().collect::<BTreeSet<_>>();

        let missing = expected
            .difference(&actual)
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        let unexpected = actual
            .difference(&expected)
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        if !missing.is_empty() || !unexpected.is_empty() {
            return Err(failure(format!(
                "catalog completeness mismatch, missing {missing:?}, unexpected {unexpected:?}"
            )));
        }

        for entry in catalog.entries() {
            let expected = &self.tests[entry.identity()];

            if entry.source() != expected.source
                || entry.constraint() != expected.constraint
                || entry.execution() != expected.execution
            {
                return Err(failure(format!(
                    "catalog source or execution metadata differs for {}: published {entry:?}, source {:?}/{:?}/{:?}",
                    entry.identity(),
                    expected.source,
                    expected.constraint,
                    expected.execution
                )));
            }
        }

        Ok(VerifiedTestCatalog {
            product: catalog.product().clone(),
            identities: catalog
                .entries()
                .iter()
                .map(|entry| entry.identity().to_string())
                .collect(),
            digest,
        })
    }
}

fn source_test_constraint(
    compilation: &Compilation,
    anchor: SyntaxAnchor,
) -> TestExecutionConstraint {
    let directive = anchor
        .find_descendant::<TestDirectiveSyntax>(compilation.syntax_tree())
        .expect("test directive anchor must resolve in its source syntax");

    // The checked catalog owns directive validation. Its only nonempty valid
    // argument list is (serial), independent of whitespace and comments.
    if directive
        .directive_argument_list()
        .is_some_and(|arguments| arguments.directive_arguments().next().is_some())
    {
        TestExecutionConstraint::Serial
    } else {
        TestExecutionConstraint::Parallel
    }
}

impl VerifiedTestCatalog {
    pub(super) fn identities(&self) -> &[String] {
        &self.identities
    }

    pub(super) fn focused<'catalog>(
        &'catalog self,
        declarations: &[&str],
    ) -> Result<Vec<&'catalog str>, BuildError> {
        let expected = declarations
            .iter()
            .map(|declaration| {
                format!(
                    "{}/{}::{declaration}",
                    self.product.package().as_str(),
                    self.product.name()
                )
            })
            .collect::<BTreeSet<_>>();

        if expected.len() != declarations.len() {
            return Err(failure("focused plan repeats an intended identity"));
        }

        let selected = self
            .identities
            .iter()
            .map(String::as_str)
            .filter(|identity| expected.contains(*identity))
            .collect::<Vec<_>>();

        let actual = selected.iter().copied().collect::<BTreeSet<_>>();

        let missing = expected
            .iter()
            .filter(|identity| !actual.contains(identity.as_str()))
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            return Err(failure(format!(
                "focused plan names absent tests: {missing:?}"
            )));
        }

        Ok(selected)
    }

    pub(super) fn require_plan_coverage(&self, identities: &[&str]) -> Result<(), BuildError> {
        let actual = identities.iter().copied().collect::<BTreeSet<_>>();

        let expected = self
            .identities
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();

        if actual.len() != identities.len() || actual != expected {
            return Err(failure(format!(
                "outcome plans do not cover the catalog exactly once, missing {:?}, unexpected {:?}, repeated {}",
                expected.difference(&actual).collect::<Vec<_>>(),
                actual.difference(&expected).collect::<Vec<_>>(),
                identities.len() - actual.len()
            )));
        }

        Ok(())
    }

    pub(super) fn require_report(
        &self,
        plan: &str,
        report: &NativeTestReport,
        selected: &[&str],
    ) -> Result<(), BuildError> {
        let [product] = report.products.as_slice() else {
            return Err(failure(format!("{plan} must report one product")));
        };

        if report.format != 1 {
            return Err(failure(format!(
                "{plan} report format is {}, expected 1",
                report.format
            )));
        }

        if product.package != self.product.package().as_str()
            || product.product != self.product.name()
        {
            return Err(failure(format!(
                "{plan} reports product {}/{}, expected {}",
                product.package, product.product, self.product
            )));
        }

        let digest = lowercase_hex(&self.digest.bytes());

        if product.catalog_digest != digest {
            return Err(failure(format!(
                "{plan} reports catalog digest {}, expected {digest}",
                product.catalog_digest
            )));
        }

        let actual = product
            .tests
            .iter()
            .map(|test| test.identity.as_str())
            .collect::<Vec<_>>();

        let expected = selected;

        if actual != expected {
            return Err(failure(format!(
                "{plan} did not execute its exact selected identities once in catalog order, expected {expected:?}, actual {actual:?}"
            )));
        }

        let discovered = self.identities.len();

        if report.selection.discovered != discovered
            || report.selection.selected != selected.len()
            || report.selection.filtered_out != discovered - selected.len()
        {
            return Err(failure(format!(
                "{plan} expected {discovered}/{}/{} discovered/selected/filtered, actual {}/{}/{}",
                selected.len(),
                discovered - selected.len(),
                report.selection.discovered,
                report.selection.selected,
                report.selection.filtered_out
            )));
        }

        let passed = product
            .tests
            .iter()
            .filter(|test| matches!(test.outcome, NativeOutcome::Passed))
            .count();

        if report.summary.passed != passed || report.summary.failed != selected.len() - passed {
            return Err(failure(format!(
                "{plan} expected {passed}/{} passed/failed outcomes, actual {}/{}",
                selected.len() - passed,
                report.summary.passed,
                report.summary.failed
            )));
        }

        Ok(())
    }
}

fn failure(message: impl Into<String>) -> BuildError {
    BuildError::conformance("native test completeness", message)
}

#[cfg(test)]
mod tests {
    use bray_compilation::{
        Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
    };
    use bray_project::load_standard_library_project_graph;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind, TestResultShape};
    use bray_target::NativeTarget;
    use bray_test_protocol::{TestCatalog, TestEntryMetadata, encode_test_catalog};

    use super::SourceTestInventory;

    fn fixture_compilation(
        sources: &[&str],
        target: NativeTarget,
    ) -> (Compilation, ProductIdentity) {
        let package = PackageIdentity::try_new("fixture").expect("fixture package");
        let product = ProductIdentity::try_new(package.clone(), "tests").expect("fixture product");

        let sources = sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                SourceInput::virtual_text(
                    SourceIdentity::new(u32::try_from(index).expect("source index")),
                    format!("source-{index}"),
                    SourceVersion::new(0),
                    *source,
                )
            })
            .collect();

        let compilation = Compilation::load(CompilationRequest::with_options(
            package,
            sources,
            CompilationOptions::new(
                WorkerBudget::default(),
                ProductKind::Test,
                SelectedTarget::for_native(target),
            ),
        ))
        .expect("source compilation loads");

        (compilation, product)
    }

    fn try_inventory(
        sources: &[&str],
        target: NativeTarget,
    ) -> Result<SourceTestInventory, super::BuildError> {
        let (compilation, product) = fixture_compilation(sources, target);

        SourceTestInventory::from_compilation(&compilation, product)
    }

    fn inventory(sources: &[&str], target: NativeTarget) -> SourceTestInventory {
        try_inventory(sources, target).expect("source inventory loads")
    }

    fn entries(inventory: &SourceTestInventory) -> Vec<TestEntryMetadata> {
        inventory
            .tests
            .iter()
            .map(|(identity, test)| {
                TestEntryMetadata::new(
                    identity.clone(),
                    test.source,
                    test.execution,
                    test.constraint,
                    TestResultShape::Unit,
                    None,
                )
            })
            .collect()
    }

    fn catalog(inventory: &SourceTestInventory, entries: Vec<TestEntryMetadata>) -> Vec<u8> {
        let catalog =
            TestCatalog::try_new(inventory.product.clone(), entries).expect("fixture catalog");

        encode_test_catalog(&catalog)
            .expect("fixture catalog encoding")
            .0
    }

    #[test]
    fn source_inventory_retains_split_modules_and_target_exclusions() {
        let sources = [
            "module checks;\n@test func first() {}\nfunc helper() {}\n",
            "@test module checks;\n@test(serial) async func second() {}\n@target(target.identity.SYSTEM == \"windows\") module checks { @test func windows() {} }\n",
        ];

        let linux = inventory(&sources, NativeTarget::X86_64LinuxGnu);

        let names = linux
            .tests
            .keys()
            .map(|identity| identity.declaration().name().as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, ["first", "second"]);
        assert_eq!(linux.excluded.len(), 1);

        let bytes = catalog(&linux, entries(&linux));

        assert!(linux.verify(&bytes).is_ok());

        let windows = inventory(&sources, NativeTarget::X86_64WindowsMsvc);

        assert!(windows.excluded.is_empty());

        assert!(
            windows
                .tests
                .keys()
                .any(|identity| identity.declaration().name().as_str() == "windows")
        );
    }

    #[test]
    fn target_alternatives_keep_distinct_source_occurrences_for_the_same_identity() {
        let source = inventory(
            &[
                "@target(target.identity.SYSTEM == \"linux\") module checks; @test func selected() {}",
                "@target(target.identity.SYSTEM == \"windows\") module checks; @test func selected() {}",
            ],
            NativeTarget::X86_64LinuxGnu,
        );

        let [(identity, excluded)] = source.excluded.as_slice() else {
            panic!("one alternate source declaration must be excluded");
        };

        let included = source.tests.get(identity).expect("active alternate");

        assert_ne!(included.source.span().source_id(), excluded.source_id());

        let bytes = catalog(&source, entries(&source));

        assert!(source.verify(&bytes).is_ok());
    }

    #[test]
    fn omitted_source_module_cannot_hide_behind_matching_catalog_and_report_totals() {
        let source = inventory(
            &[
                "module first; @test func one() {}",
                "module second; @test func two() {}",
            ],
            NativeTarget::X86_64LinuxGnu,
        );

        let mut published = entries(&source);

        published.pop();

        let bytes = catalog(&source, published);

        let error = source
            .verify(&bytes)
            .err()
            .expect("missing module must fail")
            .to_string();

        assert!(error.contains("missing"));
        assert!(error.contains("second.two"));
    }

    #[test]
    fn a_declared_test_addition_needs_no_fixed_total() {
        let source = inventory(
            &["module checks; @test func original() {} @test func added() {}"],
            NativeTarget::X86_64LinuxGnu,
        );

        let bytes = catalog(&source, entries(&source));
        let verified = source.verify(&bytes).expect("both declared tests verified");

        assert_eq!(
            verified.identities(),
            [
                "fixture/tests::checks.added",
                "fixture/tests::checks.original"
            ]
        );
    }

    #[test]
    fn all_native_target_inventories_account_for_the_windows_only_file_test() {
        let root = crate::workspace::root()
            .expect("workspace root")
            .join("standard-library");

        let graph = load_standard_library_project_graph(&root).expect("standard library graph");
        let package = PackageIdentity::try_new("std").expect("standard library package");

        let product = graph
            .package(&package)
            .expect("standard library")
            .products()
            .iter()
            .find(|product| product.identity().name() == "api")
            .expect("API product");

        for target in NativeTarget::ALL {
            let source =
                SourceTestInventory::load(&root, product, target).expect("real source inventory");

            let matches = source.tests.keys().any(|identity| {
                identity.declaration().name().as_str()
                    == "failed_file_close_retains_the_native_handle_for_retry"
            });

            assert_eq!(
                matches,
                target.as_str().contains("windows"),
                "{}",
                target.as_str()
            );

            let bytes = catalog(&source, entries(&source));
            let catalog = source.verify(&bytes).expect("source fixture catalog");

            for plan in [
                "startup-byte-buffer",
                "startup-output-lock",
                "api-sequential",
                "api-parallel",
                "api-filtered",
                "concurrency-model",
                "concurrency-stress",
                "no-build",
            ] {
                super::super::native::api_selection(&catalog, plan)
                    .expect("API intent names real applicable tests");
            }

            let outcomes = graph
                .package(&package)
                .expect("standard library")
                .products()
                .iter()
                .find(|product| product.identity().name() == "outcomes")
                .expect("outcomes product");

            let source = SourceTestInventory::load(&root, outcomes, target)
                .expect("outcome source inventory");

            let bytes = self::catalog(&source, entries(&source));
            let catalog = source.verify(&bytes).expect("outcome fixture catalog");

            super::super::native::outcome_plan_identities(&catalog)
                .expect("outcome oracles cover real declared tests");
        }
    }

    fn verified(sources: &[&str]) -> super::VerifiedTestCatalog {
        let source = inventory(sources, NativeTarget::X86_64LinuxGnu);
        let bytes = catalog(&source, entries(&source));

        source.verify(&bytes).expect("fixture catalog verifies")
    }

    fn report(
        catalog: &super::VerifiedTestCatalog,
        selected: &[&str],
    ) -> crate::native_test_report::NativeTestReport {
        let tests = selected.iter().map(|identity| serde_json::json!({
            "identity": identity, "outcome": {"kind": "passed"},
            "stdout": {"policy":"captured", "bytes":[], "truncated":false, "discarded_byte_count":0, "failure":null},
            "stderr": {"policy":"captured", "bytes":[], "truncated":false, "discarded_byte_count":0, "failure":null}
        })).collect::<Vec<_>>();

        serde_json::from_value(serde_json::json!({
            "format":1,
            "build":{"reused":false,"compilation":true,"emission":true,"linking":true,"products":[]},
            "selection":{"discovered":catalog.identities.len(),"selected":selected.len(),"filtered_out":catalog.identities.len()-selected.len()},
            "products":[{"package":catalog.product.package().as_str(),"product":catalog.product.name(),"catalog_digest":super::lowercase_hex(&catalog.digest.bytes()),"tests":tests}],
            "summary":{"passed":selected.len(),"failed":0}
        })).expect("fixture report decodes")
    }

    #[test]
    fn unexpected_same_count_catalog_identity_fails_completeness() {
        let source = inventory(
            &["module checks; @test func expected() {}"],
            NativeTarget::X86_64LinuxGnu,
        );

        let original = entries(&source).pop().expect("one entry");

        let wrong = bray_test_protocol::TestIdentity::new(
            source.product.clone(),
            bray_test_protocol::TestDeclarationPath::new(
                original.identity().declaration().module().clone(),
                bray_symbols::SymbolName::try_new("substituted").expect("name"),
            ),
        );

        let wrong = TestEntryMetadata::new(
            wrong,
            original.source(),
            original.execution(),
            original.constraint(),
            TestResultShape::Unit,
            None,
        );

        let bytes = catalog(&source, vec![wrong]);

        let error = source
            .verify(&bytes)
            .err()
            .expect("substituted catalog must fail")
            .to_string();

        assert!(error.contains("checks.expected"));
        assert!(error.contains("checks.substituted"));
    }

    #[test]
    fn duplicate_encoded_catalog_identity_is_rejected_by_the_protocol_owner() {
        let source = inventory(
            &["module checks; @test func one() {} @test func two() {}"],
            NativeTarget::X86_64LinuxGnu,
        );

        let mut bytes = catalog(&source, entries(&source));

        let index = bytes
            .windows(3)
            .position(|window| window == b"two")
            .expect("encoded second name");

        bytes[index..index + 3].copy_from_slice(b"one");

        let error = source
            .verify(&bytes)
            .err()
            .expect("duplicate catalog must fail")
            .to_string();

        assert!(error.contains("duplicate published test fixture/tests::checks.one"));
    }

    #[test]
    fn zero_catalog_and_wrong_source_or_serial_metadata_are_rejected() {
        for change in ["empty", "source", "constraint", "execution"] {
            let source = inventory(
                &["module checks; @test(serial) func expected() {}"],
                NativeTarget::X86_64LinuxGnu,
            );

            let original = entries(&source).pop().expect("one entry");

            let entries = if change == "empty" {
                Vec::new()
            } else {
                let anchor = original.source();

                vec![TestEntryMetadata::new(
                    original.identity().clone(),
                    if change == "source" {
                        bray_test_protocol::TestSourceAnchor::new(
                            anchor.package(),
                            anchor.span(),
                            SourceVersion::new(1),
                        )
                    } else {
                        anchor
                    },
                    if change == "execution" {
                        bray_symbols::CallableExecution::Asynchronous
                    } else {
                        original.execution()
                    },
                    if change == "constraint" {
                        bray_symbols::TestExecutionConstraint::Parallel
                    } else {
                        original.constraint()
                    },
                    TestResultShape::Unit,
                    None,
                )]
            };

            let bytes = catalog(&source, entries);

            assert!(source.verify(&bytes).is_err(), "{change}");
        }
    }

    #[test]
    fn focused_startup_rejects_wrong_product_digest_and_same_count_substitution() {
        let catalog =
            verified(&["module checks; @test func expected() {} @test func unrelated() {}"]);

        let selected = catalog
            .focused(&["checks.expected"])
            .expect("focused selection");

        assert!(
            catalog
                .require_report("focused", &report(&catalog, &selected), &selected)
                .is_ok()
        );

        for change in ["product", "digest", "substitution", "unexecuted"] {
            let mut report = report(&catalog, &selected);

            match change {
                "product" => report.products[0].product = "other".to_owned(),
                "digest" => report.products[0].catalog_digest = "0".repeat(64),
                "substitution" => {
                    report.products[0].tests[0].identity =
                        "fixture/tests::checks.unrelated".to_owned()
                }
                "unexecuted" => report.products[0].tests.clear(),
                _ => unreachable!(),
            }

            assert!(
                catalog
                    .require_report("focused", &report, &selected)
                    .is_err(),
                "{change}"
            );
        }
    }

    #[test]
    fn an_unintended_focused_match_fails_despite_valid_source_and_catalog() {
        let catalog =
            verified(&["module checks; @test func wanted() {} @test func wanted_extra() {}"]);

        let intended = catalog.focused(&["checks.wanted"]).expect("focused intent");

        let actual = report(
            &catalog,
            &catalog
                .identities()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );

        assert!(
            catalog
                .require_report("focused", &actual, &intended)
                .is_err()
        );
    }

    #[test]
    fn complete_plans_reject_duplicate_missing_and_reordered_results() {
        let catalog = verified(&["module checks; @test func alpha() {} @test func beta() {}"]);

        let selected = catalog
            .identities()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        for change in ["duplicate", "missing", "reordered", "summary", "selection"] {
            let mut report = report(&catalog, &selected);

            match change {
                "duplicate" => report.products[0].tests[1].identity = selected[0].to_owned(),
                "missing" => {
                    report.products[0].tests.pop();
                }
                "reordered" => report.products[0].tests.reverse(),
                "summary" => report.summary.passed = 1,
                "selection" => report.selection.filtered_out = 1,
                _ => unreachable!(),
            }

            assert!(
                catalog
                    .require_report("complete", &report, &selected)
                    .is_err(),
                "{change}"
            );
        }
    }

    #[test]
    fn outcome_plan_union_rejects_omitted_duplicate_and_unexpected_tests() {
        let catalog = verified(&["module checks; @test func alpha() {} @test func beta() {}"]);

        let mut identities = catalog
            .identities()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        assert!(catalog.require_plan_coverage(&identities).is_ok());
        identities.pop();
        assert!(catalog.require_plan_coverage(&identities).is_err());
        identities.push(identities[0]);
        assert!(catalog.require_plan_coverage(&identities).is_err());
        identities[1] = "fixture/tests::checks.unexpected";
        assert!(catalog.require_plan_coverage(&identities).is_err());
    }
    #[test]
    fn zero_discovery_and_duplicate_declared_test_identities_fail_with_context() {
        let error = try_inventory(
            &["module checks; func helper() {}"],
            NativeTarget::X86_64LinuxGnu,
        )
        .err()
        .expect("zero discovery fails")
        .to_string();

        assert!(error.contains("no applicable declared tests"));

        let error = try_inventory(
            &[
                "module checks; @test func repeated() {}",
                "module checks; @test func repeated() {}",
            ],
            NativeTarget::X86_64LinuxGnu,
        )
        .err()
        .expect("duplicate source test fails")
        .to_string();

        assert!(
            error.contains("duplicate source test fixture/tests::checks.repeated"),
            "{error}"
        );
    }
    #[test]
    fn raw_source_expectations_match_actual_checked_catalog_metadata() {
        let (compilation, product) = fixture_compilation(
            &[
                "module checks; @test func ordinary() {}",
                "@test module checks; @test(serial) async func asynchronous() {}",
            ],
            NativeTarget::X86_64LinuxGnu,
        );

        let source = SourceTestInventory::from_compilation(&compilation, product.clone())
            .expect("source inventory");

        let discovery = compilation
            .test_discovery(product)
            .expect("checked discovery");

        assert!(
            discovery.diagnostics().is_empty(),
            "{:?}",
            discovery.diagnostics()
        );

        let bytes = encode_test_catalog(discovery.value().catalog())
            .expect("checked catalog encoding")
            .0;

        assert!(source.verify(&bytes).is_ok());
    }
}
