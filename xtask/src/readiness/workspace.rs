use crate::workspace::collect_rust_files;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use quote::ToTokens;
use serde::de::DeserializeOwned;
use syn::visit::Visit;

use super::dependencies::{CargoMetadata, PackageDependencies};

pub(super) struct RustTest {
    name: String,
    qualified_name: String,
    path: String,
    body: String,
    asserted_diagnostic_kinds: BTreeSet<String>,
}

impl RustTest {
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn qualified_name(&self) -> &str {
        &self.qualified_name
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn body(&self) -> &str {
        &self.body
    }

    pub(super) fn asserted_diagnostic_kinds(&self) -> &BTreeSet<String> {
        &self.asserted_diagnostic_kinds
    }
}

pub(super) struct RustWorkspace {
    root: PathBuf,
    files: BTreeMap<String, String>,
    tests: Vec<RustTest>,
    parser_functions: Vec<String>,
    enums: BTreeMap<String, Vec<String>>,
    dependencies: CargoMetadata,
}

impl RustWorkspace {
    pub(super) fn load(root: PathBuf) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| {
            format!(
                "could not resolve readiness workspace {}: {error}",
                root.display()
            )
        })?;

        let dependencies = CargoMetadata::load(&root)?;
        let mut paths = Vec::new();

        collect_rust_files(&root.join("crates"), &mut paths)?;

        paths.sort();

        let mut files = BTreeMap::new();
        let mut tests = Vec::new();
        let mut parser_functions = Vec::new();
        let mut enums: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for path in paths {
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| format!("{} is outside the workspace", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");

            let contents = std::fs::read_to_string(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;

            let file = syn::parse_file(&contents)
                .map_err(|error| format!("could not parse {relative}: {error}"))?;

            for item in &file.items {
                let syn::Item::Enum(item) = item else {
                    continue;
                };

                enums.entry(item.ident.to_string()).or_default().extend(
                    item.variants
                        .iter()
                        .map(|variant| variant.ident.to_string()),
                );
            }

            SourceCollector {
                path: &relative,
                modules: source_module_path(&relative),
                test_only: false,
                tests: &mut tests,
                parser_functions: &mut parser_functions,
            }
            .visit_file(&file);

            files.insert(relative, contents);
        }

        for variants in enums.values_mut() {
            variants.sort();
        }

        Ok(Self {
            root,
            files,
            tests,
            parser_functions,
            enums,
            dependencies,
        })
    }

    pub(super) fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub(super) fn tests(&self) -> &[RustTest] {
        &self.tests
    }

    pub(super) fn parser_functions(&self) -> &[String] {
        &self.parser_functions
    }

    pub(super) fn executable_test_names(&self, category: &str) -> Result<BTreeSet<&str>, String> {
        self.tests
            .iter()
            .map(|test| {
                if test.path().is_empty() || test.body().is_empty() {
                    Err(format!(
                        "executable {category} test {} has no source body",
                        test.name()
                    ))
                } else {
                    Ok(test.name())
                }
            })
            .collect()
    }

    pub(super) fn enum_variants(&self, name: &str) -> Result<&[String], String> {
        self.enums
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("could not find enum {name} in the Rust workspace"))
    }

    pub(super) fn contains_source(&self, anchor: &str) -> bool {
        self.files
            .values()
            .any(|contents| contents.contains(anchor))
    }

    pub(super) fn read_fixture<T: DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        let path = self
            .root
            .join("xtask")
            .join("fixtures")
            .join("readiness")
            .join(name);

        let contents = std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;

        serde_json::from_str(&contents)
            .map_err(|error| format!("could not decode {}: {error}", path.display()))
    }

    pub(super) fn read_text(&self, relative: &str) -> Result<String, String> {
        let path = self.root.join(relative);

        std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))
    }

    pub(super) fn package_dependencies(
        &self,
        relative: &str,
    ) -> Result<&PackageDependencies, String> {
        self.dependencies.package(&self.root.join(relative))
    }
}

pub(super) fn is_fixture_anchor(value: &str) -> bool {
    let value = value.trim();

    !value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "none" | "pending" | "tbd" | "todo"
        )
}

pub(super) fn require_ordered_names<'name>(
    actual: impl IntoIterator<Item = &'name str>,
    expected: &[&str],
    category: &str,
) -> Result<(), String> {
    let actual = actual.into_iter().collect::<Vec<_>>();

    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{category} drifted: expected {expected:?}, found {actual:?}"
        ))
    }
}

pub(super) fn require_unique_names<'name>(
    actual: impl IntoIterator<Item = &'name str>,
    expected: &[&str],
    category: &str,
) -> Result<(), String> {
    let actual = actual.into_iter().collect::<Vec<_>>();
    let unique = actual.iter().copied().collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();

    if actual.len() != unique.len() {
        return Err(format!("{category} contains duplicate names"));
    }

    if unique == expected {
        Ok(())
    } else {
        Err(format!(
            "{category} drifted: expected {expected:?}, found {unique:?}"
        ))
    }
}

pub(super) fn require_executable_source_contracts<'row>(
    rows: impl IntoIterator<Item = (&'row str, &'row str, &'row str)>,
    workspace: &RustWorkspace,
    category: &str,
) -> Result<(), String> {
    let tests = workspace.executable_test_names(category)?;
    let mut names = BTreeSet::new();

    for (name, production, test) in rows {
        if !names.insert(name) {
            return Err(format!("{category} coverage fixture repeats {name}"));
        }

        if !is_fixture_anchor(production) || !workspace.contains_source(production) {
            return Err(format!(
                "missing {category} production anchor for {name}: {production}"
            ));
        }

        if !is_fixture_anchor(test) || !tests.contains(test) {
            return Err(format!(
                "missing executable {category} test for {name}: {test}"
            ));
        }
    }

    Ok(())
}

struct SourceCollector<'input, 'output> {
    path: &'input str,
    modules: Vec<String>,
    test_only: bool,
    tests: &'output mut Vec<RustTest>,
    parser_functions: &'output mut Vec<String>,
}

impl<'syntax> Visit<'syntax> for SourceCollector<'_, '_> {
    fn visit_item_mod(&mut self, module: &'syntax syn::ItemMod) {
        let test_only = self.test_only;

        self.test_only |= is_test_only(&module.attrs);

        self.modules
            .push(module.ident.to_string().trim_start_matches("r#").to_owned());

        syn::visit::visit_item_mod(self, module);
        self.modules.pop();
        self.test_only = test_only;
    }

    fn visit_item_fn(&mut self, function: &'syntax syn::ItemFn) {
        if function
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"))
        {
            let asserted_diagnostic_kinds = asserted_diagnostic_kinds(&function.block);

            self.tests.push(RustTest {
                name: function.sig.ident.to_string(),
                qualified_name: qualified_name(&self.modules, &function.sig.ident.to_string()),
                path: self.path.to_owned(),
                body: function.block.to_token_stream().to_string(),
                asserted_diagnostic_kinds,
            });
        } else if self.path.starts_with("crates/bray-parser/src/")
            && !self.test_only
            && !is_test_only(&function.attrs)
        {
            self.parser_functions.push(qualified_name(
                &self.modules,
                &function.sig.ident.to_string(),
            ));
        }
    }

    fn visit_item_impl(&mut self, implementation: &'syntax syn::ItemImpl) {
        if !self.path.starts_with("crates/bray-parser/src/")
            || self.test_only
            || is_test_only(&implementation.attrs)
        {
            return;
        }

        let syn::Type::Path(owner) = implementation.self_ty.as_ref() else {
            return;
        };

        if !owner.path.is_ident("Parser") {
            return;
        }

        for item in &implementation.items {
            if let syn::ImplItem::Fn(function) = item
                && !is_test_only(&function.attrs)
            {
                self.parser_functions.push(qualified_name(
                    &self.modules,
                    &format!("Parser::{}", function.sig.ident),
                ));
            }
        }
    }
}

fn source_module_path(path: &str) -> Vec<String> {
    let relative = path
        .split_once("/src/")
        .or_else(|| path.split_once("/tests/"))
        .map_or(path, |(_, source)| source);

    relative
        .trim_end_matches(".rs")
        .split('/')
        .filter(|part| !matches!(*part, "lib" | "main" | "mod"))
        .map(str::to_owned)
        .collect()
}

fn qualified_name(modules: &[String], name: &str) -> String {
    modules
        .iter()
        .map(String::as_str)
        .chain([name])
        .collect::<Vec<_>>()
        .join("::")
}

fn is_test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
    })
}

fn asserted_diagnostic_kinds(block: &syn::Block) -> BTreeSet<String> {
    let mut collector = DiagnosticAssertionCollector::default();

    collector.visit_block(block);

    collector
        .asserted
        .difference(&collector.fabricated)
        .cloned()
        .collect()
}

#[derive(Default)]
struct DiagnosticAssertionCollector {
    asserted: BTreeSet<String>,
    fabricated: BTreeSet<String>,
}

impl<'syntax> Visit<'syntax> for DiagnosticAssertionCollector {
    fn visit_expr_call(&mut self, call: &'syntax syn::ExprCall) {
        let Some(path) = expression_path(&call.func) else {
            syn::visit::visit_expr_call(self, call);

            return;
        };

        if path_ends_with(path, &["assert_goal_state_diagnostic_kind"])
            && call.args.len() == 2
            && let Some(kind) = diagnostic_kind(call.args.iter().nth(1))
        {
            self.asserted.insert(kind);
        }

        if path_ends_with(path, &["Diagnostic", "new"])
            && let Some(kind) = diagnostic_kind(call.args.iter().nth(1))
        {
            self.fabricated.insert(kind);
        }

        syn::visit::visit_expr_call(self, call);
    }
}

fn expression_path(expression: &syn::Expr) -> Option<&syn::Path> {
    let syn::Expr::Path(expression) = expression else {
        return None;
    };

    Some(&expression.path)
}

fn diagnostic_kind(expression: Option<&syn::Expr>) -> Option<String> {
    let path = expression_path(expression?)?;
    let mut segments = path.segments.iter().rev();
    let variant = segments.next()?.ident.to_string();

    (segments.next()?.ident == "DiagnosticKind").then_some(variant)
}

fn path_ends_with(path: &syn::Path, expected: &[&str]) -> bool {
    path.segments
        .iter()
        .rev()
        .zip(expected.iter().rev())
        .all(|(actual, expected)| actual.ident == *expected)
        && path.segments.len() >= expected.len()
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::BTreeSet;

    use syn::visit::Visit;

    use super::{RustWorkspace, SourceCollector, asserted_diagnostic_kinds, source_module_path};

    #[test]
    fn parser_anchors_keep_module_owners_and_ignore_non_executable_mentions() {
        let (functions, tests) = parsed_parser_anchors(
            r#"
            const MENTION: &str = "fn invented() {}";

            struct Parser;
            struct Other;

            impl Parser {
                fn source() {}
            }

            impl Other {
                fn source() {}
            }

            fn helper() {}

            #[cfg(test)]
            fn fixture_only() {}

            #[cfg(test)]
            mod tests {
                fn helper_without_test() {}

                #[test]
                fn real_case() {}

                mod nested {
                    #[test]
                    fn real_case() {}
                }
            }
            "#,
        );

        assert_eq!(
            functions,
            ["parser::state::Parser::source", "parser::state::helper"]
        );

        assert_eq!(
            tests,
            [
                "parser::state::tests::real_case",
                "parser::state::tests::nested::real_case"
            ],
        );
    }

    #[test]
    fn integration_test_anchors_keep_their_harness_owner() {
        assert_eq!(
            source_module_path("crates/bray-parser/tests/lexer_conformance.rs"),
            ["lexer_conformance"]
        );
    }

    pub(in crate::readiness) fn parsed_parser_anchors(source: &str) -> (Vec<String>, Vec<String>) {
        let file = syn::parse_file(source).expect("parser source fixture");
        let path = "crates/bray-parser/src/parser/state.rs";
        let mut tests = Vec::new();
        let mut functions = Vec::new();

        SourceCollector {
            path,
            modules: source_module_path(path),
            test_only: false,
            tests: &mut tests,
            parser_functions: &mut functions,
        }
        .visit_file(&file);

        (
            functions,
            tests.into_iter().map(|test| test.qualified_name).collect(),
        )
    }

    pub(in crate::readiness) fn dependency_workspace(
        packages: &[(&str, &str)],
    ) -> (tempfile::TempDir, RustWorkspace) {
        let directory = tempfile::tempdir().expect("readiness dependency workspace");

        std::fs::write(
            directory.path().join("Cargo.toml"),
            r#"
            [workspace]
            members = ["crates/*"]
            resolver = "3"

            [workspace.dependencies]
            bray-ir = "1"
            mir = { package = "bray-ir", version = "1" }
        "#,
        )
        .expect("workspace manifest");

        for (name, dependencies) in packages {
            let package = directory.path().join("crates").join(name);

            std::fs::create_dir_all(package.join("src")).expect("package source directory");
            std::fs::write(package.join("src/lib.rs"), "").expect("package source");

            std::fs::write(
                package.join("Cargo.toml"),
                format!(
                    r#"
                [package]
                name = "{name}"
                version = "0.0.0"
                edition = "2024"

                {dependencies}
            "#
                ),
            )
            .expect("package manifest");
        }

        let workspace = RustWorkspace::load(directory.path().to_owned())
            .expect("readiness workspace must load without dependency resolution");

        (directory, workspace)
    }

    #[test]
    fn diagnostic_assertions_follow_exact_structured_calls() {
        assert_eq!(
            assertions(
                "fn test() { assert_goal_state_diagnostic_kind(&bag, DiagnosticKind::Exact); }"
            ),
            BTreeSet::from(["Exact".to_owned()]),
        );

        assert_eq!(
            assertions(
                r#"
                fn test() {
                    assert_goal_state_diagnostic_kind(
                        produced.diagnostics(),
                        DiagnosticKind::Multiline,
                    );
                }
                "#,
            ),
            BTreeSet::from(["Multiline".to_owned()]),
        );
    }

    #[test]
    fn diagnostic_assertions_reject_mentions_wrong_kinds_and_fabricated_records() {
        assert!(assertions(
            r#"fn test() { let mention = "assert_goal_state_diagnostic_kind(DiagnosticKind::Mention)"; }"#
        )
        .is_empty());

        assert_eq!(
            assertions(
                "fn test() { assert_goal_state_diagnostic_kind(&bag, DiagnosticKind::Actual); }"
            ),
            BTreeSet::from(["Actual".to_owned()]),
        );

        assert!(
            assertions(
                r#"
            fn test() {
                let bag = DiagnosticBag::single(Diagnostic::new(
                    DiagnosticId::new(0),
                    DiagnosticKind::Fabricated,
                    SeverityKind::Error,
                ));
                assert_goal_state_diagnostic_kind(&bag, DiagnosticKind::Fabricated);
            }
            "#,
            )
            .is_empty()
        );
    }

    fn assertions(source: &str) -> BTreeSet<String> {
        let function = syn::parse_str::<syn::ItemFn>(source)
            .unwrap_or_else(|error| panic!("test function must parse: {error}"));

        asserted_diagnostic_kinds(&function.block)
    }
}
