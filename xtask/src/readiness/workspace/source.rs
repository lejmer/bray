use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use quote::ToTokens;
use syn::visit::Visit;

use super::super::dependencies::CargoTarget;

pub(in crate::readiness) struct RustTest {
    name: String,
    qualified_name: String,
    path: String,
    body: String,
    asserted_diagnostic_kinds: BTreeSet<String>,
}

impl RustTest {
    pub(in crate::readiness) fn name(&self) -> &str {
        &self.name
    }

    pub(in crate::readiness) fn qualified_name(&self) -> &str {
        &self.qualified_name
    }

    pub(in crate::readiness) fn path(&self) -> &str {
        &self.path
    }

    pub(in crate::readiness) fn body(&self) -> &str {
        &self.body
    }

    pub(in crate::readiness) fn asserted_diagnostic_kinds(&self) -> &BTreeSet<String> {
        &self.asserted_diagnostic_kinds
    }
}

pub(super) struct SourceCollector<'input, 'output> {
    path: &'input str,
    modules: Vec<String>,
    production_enabled: bool,
    tests: &'output mut Vec<RustTest>,
    parser_functions: &'output mut Vec<String>,
    sources: Option<&'input BTreeMap<String, syn::File>>,
    active_files: Vec<&'input str>,
    file_module_depth: usize,
    is_target_root: bool,
    tests_enabled: bool,
    error: Option<String>,
}

pub(super) fn collect_parser_sources(
    root: &Path,
    targets: &[CargoTarget],
    sources: &BTreeMap<String, syn::File>,
    tests: &mut Vec<RustTest>,
    functions: &mut Vec<String>,
) -> Result<(), String> {
    for target in targets.iter().filter(|target| {
        target
            .kind
            .iter()
            .any(|kind| matches!(kind.as_str(), "lib" | "bin" | "test"))
    }) {
        let relative = target
            .src_path
            .strip_prefix(root)
            .map_err(|_| format!("parser target {} is outside the workspace", target.name))?
            .to_string_lossy()
            .replace('\\', "/");

        let (path, file) = sources
            .get_key_value(&relative)
            .ok_or_else(|| format!("missing parser target source: {relative}"))?;

        let integration = target.kind.iter().any(|kind| kind == "test");

        let modules = if integration {
            vec![target.name.clone()]
        } else {
            Vec::new()
        };

        let mut collector = SourceCollector::new(path, modules, tests, functions);

        collector.sources = Some(sources);
        collector.production_enabled = !integration;
        collector.tests_enabled = target.test;
        collector.visit_file(file);

        if let Some(error) = collector.error {
            return Err(error);
        }
    }

    Ok(())
}

impl<'input, 'output> SourceCollector<'input, 'output> {
    pub(super) fn new(
        path: &'input str,
        modules: Vec<String>,
        tests: &'output mut Vec<RustTest>,
        parser_functions: &'output mut Vec<String>,
    ) -> Self {
        let file_module_depth = modules.len();

        Self {
            path,
            modules,
            production_enabled: true,
            tests,
            parser_functions,
            sources: None,
            active_files: vec![path],
            file_module_depth,
            is_target_root: true,
            tests_enabled: true,
            error: None,
        }
    }

    fn module_file(
        &self,
        module: &syn::ItemMod,
    ) -> Result<(&'input str, &'input syn::File), String> {
        let path = Path::new(self.path);
        let parent = path.parent().expect("Rust source has a parent directory");
        let mut directory = parent.to_path_buf();

        if !self.is_target_root && path.file_name().is_none_or(|name| name != "mod.rs") {
            directory.push(path.file_stem().expect("Rust module has a file stem"));
        }

        let inline = &self.modules[self.file_module_depth..];

        directory.extend(inline);

        let explicit_path = module
            .attrs
            .iter()
            .find(|attribute| attribute.path().is_ident("path"));

        let candidates = if let Some(attribute) = explicit_path {
            let syn::Meta::NameValue(value) = &attribute.meta else {
                return Err(format!(
                    "invalid Rust module path attribute in {}",
                    self.path
                ));
            };

            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) = &value.value
            else {
                return Err(format!(
                    "Rust module path must be a string in {}",
                    self.path
                ));
            };

            if inline.is_empty() {
                directory = parent.to_path_buf();
            }

            vec![directory.join(value.value())]
        } else {
            let name = module.ident.to_string().trim_start_matches("r#").to_owned();

            vec![
                directory.join(format!("{name}.rs")),
                directory.join(name).join("mod.rs"),
            ]
        };

        let sources = self
            .sources
            .expect("parser module traversal retains parsed files");

        let matches = candidates
            .into_iter()
            .filter_map(|candidate| {
                let mut normalized = PathBuf::new();

                for component in candidate.components() {
                    match component {
                        Component::ParentDir => {
                            normalized.pop();
                        }
                        Component::CurDir => {}
                        part => normalized.push(part),
                    }
                }

                sources.get_key_value(&normalized.to_string_lossy().replace('\\', "/"))
            })
            .collect::<Vec<_>>();

        let [(path, file)] = matches.as_slice() else {
            return Err(format!(
                "parser Rust module {} in {} must resolve to one source file, found {}",
                module.ident,
                self.path,
                matches.len()
            ));
        };

        if self.active_files.contains(&path.as_str()) {
            return Err(format!(
                "cyclic parser Rust module {} in {}",
                module.ident, self.path
            ));
        }

        Ok((path.as_str(), file))
    }
}

impl<'syntax, 'input: 'syntax> Visit<'syntax> for SourceCollector<'input, '_> {
    fn visit_item(&mut self, item: &'syntax syn::Item) {
        match item {
            syn::Item::Fn(function) => self.visit_item_fn(function),
            syn::Item::Mod(module) => self.visit_item_mod(module),
            syn::Item::Impl(implementation) => self.visit_item_impl(implementation),
            _ => {}
        }
    }

    fn visit_file(&mut self, file: &'syntax syn::File) {
        let production_enabled = self.production_enabled;
        let tests_enabled = self.tests_enabled;

        self.production_enabled &= cfg_allows(&file.attrs, false);
        self.tests_enabled &= cfg_allows(&file.attrs, true);

        if self.production_enabled || self.tests_enabled {
            syn::visit::visit_file(self, file);
        }

        self.production_enabled = production_enabled;
        self.tests_enabled = tests_enabled;
    }

    fn visit_item_mod(&mut self, module: &'syntax syn::ItemMod) {
        if self.error.is_some() {
            return;
        }

        let production_enabled = self.production_enabled && cfg_allows(&module.attrs, false);
        let tests_enabled = self.tests_enabled && cfg_allows(&module.attrs, true);

        if !production_enabled && !tests_enabled {
            return;
        }

        let external = if module.content.is_none() && self.sources.is_some() {
            match self.module_file(module) {
                Ok(source) => Some(source),
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        } else {
            None
        };

        let previous_production = self.production_enabled;
        let previous_tests = self.tests_enabled;

        self.production_enabled = production_enabled;
        self.tests_enabled = tests_enabled;

        self.modules
            .push(module.ident.to_string().trim_start_matches("r#").to_owned());

        if let Some((path, file)) = external {
            let previous_path = self.path;
            let previous_depth = self.file_module_depth;
            let previous_root = self.is_target_root;

            self.path = path;
            self.file_module_depth = self.modules.len();
            self.is_target_root = false;
            self.active_files.push(path);
            self.visit_file(file);
            self.active_files.pop();
            self.path = previous_path;
            self.file_module_depth = previous_depth;
            self.is_target_root = previous_root;
        } else {
            syn::visit::visit_item_mod(self, module);
        }

        self.modules.pop();
        self.production_enabled = previous_production;
        self.tests_enabled = previous_tests;
    }

    fn visit_item_fn(&mut self, function: &'syntax syn::ItemFn) {
        if function
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"))
        {
            if !self.tests_enabled || !cfg_allows(&function.attrs, true) {
                return;
            }

            let asserted_diagnostic_kinds = asserted_diagnostic_kinds(&function.block);

            self.tests.push(RustTest {
                name: function.sig.ident.to_string(),
                qualified_name: qualified_name(&self.modules, &function.sig.ident.to_string()),
                path: self.path.to_owned(),
                body: function.block.to_token_stream().to_string(),
                asserted_diagnostic_kinds,
            });
        } else if self.path.starts_with("crates/bray-parser/src/")
            && self.production_enabled
            && cfg_allows(&function.attrs, false)
        {
            self.parser_functions.push(qualified_name(
                &self.modules,
                &function.sig.ident.to_string(),
            ));
        }
    }

    fn visit_item_impl(&mut self, implementation: &'syntax syn::ItemImpl) {
        if !self.path.starts_with("crates/bray-parser/src/")
            || !self.production_enabled
            || !cfg_allows(&implementation.attrs, false)
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
                && cfg_allows(&function.attrs, false)
            {
                self.parser_functions.push(qualified_name(
                    &self.modules,
                    &format!("Parser::{}", function.sig.ident),
                ));
            }
        }
    }
}

pub(super) fn source_module_path(path: &str) -> Vec<String> {
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

fn cfg_allows(attributes: &[syn::Attribute], test: bool) -> bool {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .all(|attribute| {
            attribute
                .parse_args::<syn::Meta>()
                .ok()
                .and_then(|condition| cfg_value(&condition, test))
                != Some(false)
        })
}

// The test harness fixes `test`; feature/target predicates remain potential source evidence.
fn cfg_value(condition: &syn::Meta, test: bool) -> Option<bool> {
    use syn::parse::Parser;

    match condition {
        syn::Meta::Path(path) if path.is_ident("test") => Some(test),
        syn::Meta::List(list) => {
            let conditions =
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                    .ok()?;

            let values = conditions
                .iter()
                .map(|condition| cfg_value(condition, test))
                .collect::<Vec<_>>();

            if list.path.is_ident("not") {
                match values.as_slice() {
                    [Some(value)] => Some(!value),
                    _ => None,
                }
            } else if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(true)
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn asserted_diagnostic_kinds(block: &syn::Block) -> BTreeSet<String> {
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
    use super::{SourceCollector, asserted_diagnostic_kinds, source_module_path};
    use std::collections::BTreeSet;
    use syn::visit::Visit;

    #[test]
    fn parser_anchors_keep_module_owners_and_ignore_non_executable_mentions() {
        let (functions, tests) = parsed_parser_anchors(
            r#"
            const MENTION: &str = "fn invented() {}";
            const INNER: () = {
                #[test]
                fn not_a_harness_entry() {}
            };

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

    pub(in crate::readiness) fn parsed_parser_anchors(source: &str) -> (Vec<String>, Vec<String>) {
        let file = syn::parse_file(source).expect("parser source fixture");
        let path = "crates/bray-parser/src/parser/state.rs";
        let mut tests = Vec::new();
        let mut functions = Vec::new();

        SourceCollector::new(path, source_module_path(path), &mut tests, &mut functions)
            .visit_file(&file);

        (
            functions,
            tests
                .into_iter()
                .map(|test| test.qualified_name().to_owned())
                .collect(),
        )
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
