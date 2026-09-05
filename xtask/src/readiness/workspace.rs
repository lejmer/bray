use crate::workspace::collect_rust_files;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use quote::ToTokens;
use serde::de::DeserializeOwned;
use syn::visit::Visit;

pub(super) struct RustTest {
    name: String,
    path: String,
    body: String,
    asserted_diagnostic_kinds: BTreeSet<String>,
}

impl RustTest {
    pub(super) fn name(&self) -> &str {
        &self.name
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
    enums: BTreeMap<String, Vec<String>>,
}

impl RustWorkspace {
    pub(super) fn load(root: PathBuf) -> Result<Self, String> {
        let mut paths = Vec::new();

        collect_rust_files(&root.join("crates"), &mut paths)?;

        paths.sort();

        let mut files = BTreeMap::new();
        let mut tests = Vec::new();
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

            TestCollector {
                path: &relative,
                tests: &mut tests,
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
            enums,
        })
    }

    pub(super) fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub(super) fn tests(&self) -> &[RustTest] {
        &self.tests
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

struct TestCollector<'input, 'output> {
    path: &'input str,
    tests: &'output mut Vec<RustTest>,
}

impl<'syntax> Visit<'syntax> for TestCollector<'_, '_> {
    fn visit_item_fn(&mut self, function: &'syntax syn::ItemFn) {
        if function
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("test"))
        {
            let asserted_diagnostic_kinds = asserted_diagnostic_kinds(&function.block);

            self.tests.push(RustTest {
                name: function.sig.ident.to_string(),
                path: self.path.to_owned(),
                body: function.block.to_token_stream().to_string(),
                asserted_diagnostic_kinds,
            });
        }
    }
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
mod tests {
    use std::collections::BTreeSet;

    use super::asserted_diagnostic_kinds;

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
