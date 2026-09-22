//! Detects conversions into configured broad failures that discard known context.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ra_ap_syntax::ast::{HasArgList, HasName};
use ra_ap_syntax::{AstNode, AstToken, NodeOrToken, SourceFile, SyntaxNode, TextRange, ast};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use super::diagnostic::{Diagnostic, Rule};
use super::source::{self, range_is_test_only, source_text, test_only_ranges};

const CATEGORY_DIRECTIVE: &str = "broad-failure";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Category {
    enum_name: String,
    has_payload: bool,
    variant_name: String,
}

pub(super) struct Policy {
    categories: BTreeSet<Category>,
}

impl Policy {
    pub(super) fn from_paths(paths: &[PathBuf]) -> Result<Self, String> {
        let results = paths
            .par_iter()
            .map(|path| {
                if source::is_test_source(path) {
                    return Ok(Vec::new());
                }

                let source = std::fs::read_to_string(path)
                    .map_err(|error| source::io_error("read", path, error))?;

                let file = SourceFile::parse(&source, ra_ap_syntax::Edition::Edition2024).tree();

                Ok(valid_categories(&source, &file))
            })
            .collect::<Vec<Result<Vec<Category>, String>>>();

        let mut categories = BTreeSet::new();

        for result in results {
            categories.extend(result?);
        }

        Ok(Self { categories })
    }

    #[cfg(test)]
    fn from_sources(sources: &[&str]) -> Self {
        let categories = sources
            .iter()
            .flat_map(|source| {
                let file = SourceFile::parse(source, ra_ap_syntax::Edition::Edition2024).tree();

                valid_categories(source, &file)
            })
            .collect();

        Self { categories }
    }

    fn category_for_path(
        &self,
        path: &str,
        aliases: &BTreeMap<String, String>,
        self_type: Option<&str>,
    ) -> Option<&Category> {
        let compact = path
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();

        let mut segments = compact.rsplit("::");

        let Some(variant_name) = segments.next() else {
            return None;
        };

        let qualifier = segments.next();

        if let Some(enum_name) = qualifier {
            let enum_name = if enum_name == "Self" {
                self_type?
            } else {
                enum_name
            };

            let enum_name = aliases.get(enum_name).map_or(enum_name, String::as_str);

            return self.categories.iter().find(|category| {
                category.enum_name == enum_name && category.variant_name == variant_name
            });
        }

        let mut matching = self
            .categories
            .iter()
            .filter(|category| category.variant_name == variant_name);

        let category = matching.next()?;

        matching.next().is_none().then_some(category)
    }

    fn contains_return_type(&self, return_type: &str) -> bool {
        self.categories.iter().any(|category| {
            return_type.contains(&category.enum_name)
                || return_type.contains(&category.variant_name)
        })
    }
}

pub(super) fn check(
    path: &Path,
    source: &str,
    file: &ast::SourceFile,
    policy: &Policy,
) -> Vec<Diagnostic> {
    let mut diagnostics = marker_diagnostics(source, file);

    if source::is_test_source(path) {
        return diagnostics;
    }

    let test_ranges = test_only_ranges(file, source);
    let aliases = failure_aliases(file, policy);

    for path_expression in file.syntax().descendants().filter_map(ast::PathExpr::cast) {
        let Some(path) = path_expression.path() else {
            continue;
        };

        let Some(category) = policy.category_for_path(
            &path.syntax().text().to_string(),
            &aliases,
            enclosing_self_type(path_expression.syntax()).as_deref(),
        ) else {
            continue;
        };

        let Some((constructor, has_payload)) = call_constructor(path_expression, category) else {
            continue;
        };

        push_constructor_diagnostic(
            source,
            policy,
            &test_ranges,
            &constructor,
            has_payload,
            &mut diagnostics,
        );
    }

    for record in file
        .syntax()
        .descendants()
        .filter_map(ast::RecordExpr::cast)
    {
        let Some(path) = record.path() else {
            continue;
        };

        let Some(category) = policy.category_for_path(
            &path.syntax().text().to_string(),
            &aliases,
            enclosing_self_type(record.syntax()).as_deref(),
        ) else {
            continue;
        };

        push_constructor_diagnostic(
            source,
            policy,
            &test_ranges,
            record.syntax(),
            category.has_payload,
            &mut diagnostics,
        );
    }

    diagnostics
}

fn failure_aliases(file: &ast::SourceFile, policy: &Policy) -> BTreeMap<String, String> {
    file.syntax()
        .descendants()
        .filter_map(ast::UseTree::cast)
        .filter_map(|tree| {
            let path = tree.path()?.syntax().text().to_string();
            let enum_name = path.rsplit("::").next()?;

            if !policy
                .categories
                .iter()
                .any(|category| category.enum_name == enum_name)
            {
                return None;
            }

            let alias = tree.rename()?.name()?.text().to_string();

            Some((alias, enum_name.to_owned()))
        })
        .collect()
}

fn enclosing_self_type(node: &SyntaxNode) -> Option<String> {
    let implementation = node.ancestors().find_map(ast::Impl::cast)?;

    let self_type = implementation
        .syntax()
        .children()
        .filter_map(ast::Type::cast)
        .last()?;

    let compact = self_type
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();

    let without_arguments = compact.split('<').next()?;

    without_arguments.rsplit("::").next().map(str::to_owned)
}

fn push_constructor_diagnostic(
    source: &str,
    policy: &Policy,
    test_ranges: &[TextRange],
    constructor: &SyntaxNode,
    has_payload: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let range = constructor.text_range();

    if range_is_test_only(range, test_ranges) {
        return;
    }

    let conversion = conversion_bindings(policy, constructor);

    let preserves_bindings = conversion.as_ref().is_some_and(|bindings| {
        !bindings.is_empty()
            && bindings
                .iter()
                .all(|binding| references_name(constructor, binding))
    });

    if has_payload && (conversion.is_none() || preserves_bindings) {
        return;
    }

    let constructor_text = source_text(source, range)
        .split(['(', '{'])
        .next()
        .unwrap_or("broad failure")
        .trim();

    diagnostics.push(
        Diagnostic::new(Rule::ContextErasingFailureConversion, range.start())
            .with_message(format!(
                "construction of configured broad failure `{constructor_text}` discards exact cause or context"
            ))
            .with_help(
                "retain the leaf failure and every known identity, path, span, target, expected value, and actual value in a typed payload, or add a justified expression-local exemption when no exact cause exists",
            )
            .for_expression(range),
    );
}

fn call_constructor(
    path_expression: ast::PathExpr,
    category: &Category,
) -> Option<(SyntaxNode, bool)> {
    let Some(parent) = path_expression.syntax().parent() else {
        return (!category.has_payload).then(|| (path_expression.syntax().clone(), false));
    };

    let Some(call) = ast::CallExpr::cast(parent) else {
        return (!category.has_payload).then(|| (path_expression.syntax().clone(), false));
    };

    let is_callee = call
        .expr()
        .is_some_and(|callee| callee.syntax() == path_expression.syntax());

    if !is_callee {
        return (!category.has_payload).then(|| (path_expression.syntax().clone(), false));
    }

    let has_payload = call
        .arg_list()
        .is_some_and(|arguments| arguments.args().next().is_some());

    Some((call.syntax().clone(), has_payload))
}

fn conversion_bindings(policy: &Policy, constructor: &SyntaxNode) -> Option<Vec<String>> {
    for ancestor in constructor.ancestors().skip(1) {
        if let Some(closure) = ast::ClosureExpr::cast(ancestor.clone()) {
            let Some(parameters) = closure.param_list() else {
                continue;
            };

            let parameters = parameters.params().collect::<Vec<_>>();

            if parameters.is_empty() {
                continue;
            }

            let bindings = parameters
                .into_iter()
                .filter_map(|parameter| parameter.pat())
                .flat_map(pattern_bindings)
                .collect::<Vec<_>>();

            let is_map_err = closure
                .syntax()
                .ancestors()
                .find_map(ast::MethodCallExpr::cast)
                .and_then(|call| call.name_ref())
                .is_some_and(|name| name.text() == "map_err");

            if is_map_err {
                return Some(bindings);
            }

            let bindings = failure_context_bindings(bindings);

            if !bindings.is_empty() {
                return Some(bindings);
            }

            continue;
        }

        if let Some(arm) = ast::MatchArm::cast(ancestor.clone())
            && let Some(pattern) = arm.pat()
        {
            let pattern_text = pattern.syntax().text().to_string();
            let pattern_bindings = pattern_bindings(pattern.clone()).collect::<Vec<_>>();

            if !pattern_bindings.is_empty()
                && (pattern_text.contains("Err(")
                    || pattern_text.contains("Error::")
                    || pattern_text.contains("Failure"))
            {
                return Some(pattern_bindings);
            }

            let bindings = failure_context_bindings(pattern_bindings.clone());

            let has_wildcard = pattern
                .syntax()
                .descendants()
                .any(|node| ast::WildcardPat::cast(node).is_some());

            if !bindings.is_empty() {
                return Some(bindings);
            }

            if has_wildcard {
                let mut bindings = pattern_bindings;

                if let Some(scrutinee) = arm
                    .syntax()
                    .ancestors()
                    .find_map(ast::MatchExpr::cast)
                    .and_then(|expression| expression.expr())
                {
                    bindings.extend(
                        scrutinee
                            .syntax()
                            .descendants()
                            .filter_map(ast::PathExpr::cast)
                            .filter_map(|expression| expression.path())
                            .map(|path| path.syntax().text().to_string())
                            .filter(|path| !path.contains("::")),
                    );
                }

                bindings.sort();
                bindings.dedup();

                return Some(bindings);
            }

            continue;
        }

        let Some(function) = ast::Fn::cast(ancestor) else {
            continue;
        };

        return function_conversion_bindings(policy, &function);
    }

    None
}

fn function_conversion_bindings(policy: &Policy, function: &ast::Fn) -> Option<Vec<String>> {
    let is_from = function.name().is_some_and(|name| name.text() == "from")
        && function
            .syntax()
            .ancestors()
            .find_map(ast::Impl::cast)
            .is_some_and(|implementation| {
                implementation.trait_().is_some_and(|trait_type| {
                    trait_type.syntax().text().to_string().contains("From")
                })
            });

    let returns_broad_failure = function.ret_type().is_some_and(|return_type| {
        policy.contains_return_type(&return_type.syntax().text().to_string())
    });

    if !is_from && !returns_broad_failure {
        return None;
    }

    let parameters = function
        .param_list()
        .into_iter()
        .flat_map(|parameters| parameters.params())
        .filter_map(|parameter| {
            let is_failure_type = parameter.ty().is_some_and(type_is_failure);

            let bindings = parameter.pat().map(pattern_bindings)?.collect::<Vec<_>>();

            Some((bindings, is_failure_type))
        })
        .collect::<Vec<_>>();

    if is_from {
        return Some(
            parameters
                .into_iter()
                .flat_map(|(bindings, _)| bindings)
                .collect(),
        );
    }

    let has_failure_input = parameters.iter().any(|(bindings, is_failure_type)| {
        *is_failure_type || bindings.iter().any(|binding| is_failure_binding(binding))
    });

    if !has_failure_input {
        return None;
    }

    let bindings = parameters
        .into_iter()
        .flat_map(|(bindings, is_failure_type)| {
            bindings
                .into_iter()
                .filter(move |binding| is_failure_type || is_failure_context(binding))
        })
        .collect();

    Some(bindings)
}

fn type_is_failure(ty: ast::Type) -> bool {
    let compact = ty
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();

    let direct = compact.trim_start_matches('&');

    if direct.starts_with("dyn") || direct.starts_with("impl") {
        return false;
    }

    let direct = direct.split('<').next().unwrap_or(direct);

    let name = direct
        .rsplit("::")
        .next()
        .unwrap_or(direct)
        .to_ascii_lowercase();

    ["cause", "error", "failure"]
        .iter()
        .any(|role| name == *role || name.ends_with(role))
}

fn is_failure_binding(binding: &str) -> bool {
    name_has_role(binding, &["cause", "error", "failure"])
}

fn failure_context_bindings(bindings: Vec<String>) -> Vec<String> {
    if !bindings.iter().any(|binding| is_failure_binding(binding)) {
        return Vec::new();
    }

    bindings
        .into_iter()
        .filter(|binding| is_failure_context(binding))
        .collect()
}

fn is_failure_context(binding: &str) -> bool {
    name_has_role(
        binding,
        &[
            "actual", "cause", "context", "error", "expected", "failure", "identity", "path",
            "span", "target",
        ],
    )
}

fn name_has_role(binding: &str, roles: &[&str]) -> bool {
    let binding = binding.to_ascii_lowercase();

    roles
        .iter()
        .any(|part| binding == *part || binding.ends_with(&format!("_{part}")))
}

fn pattern_bindings(pattern: ast::Pat) -> impl Iterator<Item = String> {
    std::iter::once(pattern.syntax().clone())
        .chain(pattern.syntax().descendants())
        .filter_map(ast::IdentPat::cast)
        .filter_map(|pattern| pattern.name())
        .map(|name| name.text().to_string())
        .filter(|name| name != "_")
}

fn references_name(node: &SyntaxNode, name: &str) -> bool {
    node.descendants()
        .filter_map(ast::NameRef::cast)
        .any(|reference| reference.text() == name)
}

fn valid_categories(source: &str, file: &SourceFile) -> Vec<Category> {
    let test_ranges = test_only_ranges(file, source);

    category_markers(source, file)
        .into_iter()
        .filter(|marker| !range_is_test_only(marker.range, &test_ranges))
        .filter_map(|marker| marker.category)
        .collect()
}

fn marker_diagnostics(source: &str, file: &SourceFile) -> Vec<Diagnostic> {
    let test_ranges = test_only_ranges(file, source);

    category_markers(source, file)
        .into_iter()
        .filter(|marker| !range_is_test_only(marker.range, &test_ranges))
        .filter_map(|marker| marker.diagnostic)
        .collect()
}

struct CategoryMarker {
    range: TextRange,
    category: Option<Category>,
    diagnostic: Option<Diagnostic>,
}

fn category_markers(source: &str, file: &SourceFile) -> Vec<CategoryMarker> {
    let syntax = file.syntax();

    syntax
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter_map(ast::Comment::cast)
        .filter_map(|comment| {
            let directive = directive_text(comment.text())?;
            let is_category = directive.starts_with(CATEGORY_DIRECTIVE);
            let exact = directive == CATEGORY_DIRECTIVE;

            is_category.then_some((comment, exact))
        })
        .map(|(comment, exact)| {
            let range = comment.syntax().text_range();
            let offset = range.start();

            if !exact {
                return CategoryMarker {
                    range,
                    category: None,
                    diagnostic: Some(
                        Diagnostic::new(Rule::InvalidFailureCategory, offset).with_message(
                            "broad failure category marker must be exactly `rust-style: broad-failure`",
                        ),
                    ),
                };
            }

            let comment_end = comment.syntax().text_range().end();

            let variant = syntax
                .descendants()
                .filter_map(ast::Variant::cast)
                .filter_map(|variant| {
                    let start = variant.name()?.syntax().text_range().start();

                    (start >= comment_end).then_some((variant, start))
                })
                .min_by_key(|(_, start)| *start)
                .filter(|(_, start)| {
                    source_text(source, TextRange::new(comment_end, *start))
                        .chars()
                        .all(char::is_whitespace)
                });

            let Some((variant, _)) = variant else {
                return CategoryMarker {
                    range,
                    category: None,
                    diagnostic: Some(
                        Diagnostic::new(Rule::InvalidFailureCategory, offset).with_message(
                            "broad failure category marker must be immediately before an enum variant",
                        ),
                    ),
                };
            };

            let Some(enum_item) = variant.syntax().ancestors().find_map(ast::Enum::cast) else {
                return CategoryMarker {
                    range,
                    category: None,
                    diagnostic: Some(Diagnostic::new(Rule::InvalidFailureCategory, offset)),
                };
            };

            CategoryMarker {
                range,
                category: enum_item
                    .name()
                    .zip(variant.name())
                    .map(|(enum_name, variant_name)| Category {
                        enum_name: enum_name.text().to_string(),
                        has_payload: variant.field_list().is_some(),
                        variant_name: variant_name.text().to_string(),
                    }),
                diagnostic: None,
            }
        })
        .collect()
}

fn directive_text(comment: &str) -> Option<&str> {
    comment
        .strip_prefix("//")?
        .trim()
        .strip_prefix("rust-style:")
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ra_ap_syntax::{Edition, SourceFile};

    use super::{Policy, check};
    use crate::diagnostic::Rule;
    use crate::exemption;

    const CATEGORY: &str = r#"
enum BroadFailure {
    // rust-style: broad-failure
    Infrastructure { cause: LeafFailure, context: Context },
    // rust-style: broad-failure
    Missing,
}
"#;

    fn rules(source: &str) -> Vec<Rule> {
        let policy = Policy::from_sources(&[CATEGORY, source]);
        let file = SourceFile::parse(source, Edition::Edition2024).tree();

        exemption::apply(
            source,
            &file,
            check(Path::new("src/example.rs"), source, &file, &policy),
        )
        .into_iter()
        .map(|diagnostic| diagnostic.rule)
        .collect()
    }

    #[test]
    fn typed_payloads_preserve_conversion_causes() {
        let source = r#"
fn preserve(
    result: Result<(), LeafFailure>,
    context: Context,
) -> Result<(), BroadFailure> {
    result.map_err(|error| BroadFailure::Infrastructure {
        cause: error,
        context,
    })
}
"#;

        assert_eq!(rules(source), []);
    }

    #[test]
    fn tuple_variant_constructor_functions_preserve_their_input() {
        let category = r#"
enum BroadFailure {
    // rust-style: broad-failure
    Infrastructure(LeafFailure),
}
"#;

        let source = r#"
fn preserve(result: Result<(), LeafFailure>) -> Result<(), BroadFailure> {
    result.map_err(BroadFailure::Infrastructure)
}
"#;

        let policy = Policy::from_sources(&[category, source]);
        let file = SourceFile::parse(source, Edition::Edition2024).tree();
        let diagnostics = check(Path::new("src/example.rs"), source, &file, &policy);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn aliased_broad_failure_types_are_checked() {
        let source = r#"
use crate::BroadFailure as Failure;

fn missing() -> Failure {
    Failure::Missing
}
"#;

        let policy = Policy::from_sources(&[CATEGORY, source]);
        let file = SourceFile::parse(source, Edition::Edition2024).tree();
        let diagnostics = check(Path::new("src/example.rs"), source, &file, &policy);

        assert_eq!(diagnostics.len(), 1);

        assert_eq!(diagnostics[0].rule, Rule::ContextErasingFailureConversion);
    }

    #[test]
    fn self_qualified_variants_use_the_enclosing_implementation_type() {
        let categories = r#"
enum FirstFailure {
    // rust-style: broad-failure
    Missing,
}

enum SecondFailure {
    // rust-style: broad-failure
    Missing,
}
"#;

        let source = r#"
impl FirstFailure {
    fn missing() -> Self {
        Self::Missing
    }
}
"#;

        let policy = Policy::from_sources(&[categories, source]);
        let file = SourceFile::parse(source, Edition::Edition2024).tree();
        let diagnostics = check(Path::new("src/example.rs"), source, &file, &policy);

        assert_eq!(diagnostics.len(), 1);

        assert_eq!(diagnostics[0].rule, Rule::ContextErasingFailureConversion);
    }

    #[test]
    fn inline_closures_and_match_arms_cannot_discard_bound_causes() {
        let source = r#"
fn inline(result: Result<(), LeafFailure>, context: Context) -> BroadFailure {
    let _ = result.map_err(|error| BroadFailure::Infrastructure {
        cause: fallback,
        context,
    });

    let _ = result.map_err(|_| BroadFailure::Infrastructure {
        cause: fallback,
        context,
    });

    match result {
        Err(error) => BroadFailure::Infrastructure {
            cause: fallback,
            context,
        },
        Ok(()) => BroadFailure::Missing,
    }
}
"#;

        assert_eq!(
            rules(source),
            [
                Rule::ContextErasingFailureConversion,
                Rule::ContextErasingFailureConversion,
                Rule::ContextErasingFailureConversion,
                Rule::ContextErasingFailureConversion,
            ]
        );
    }

    #[test]
    fn helpers_from_implementations_and_option_conversions_are_covered() {
        let source = r#"
fn erase(e: LeafFailure, context: Context) -> BroadFailure {
    BroadFailure::Infrastructure {
        cause: fallback,
        context,
    }
}

impl From<LeafFailure> for BroadFailure {
    fn from(error: LeafFailure) -> Self {
        BroadFailure::Infrastructure {
            cause: fallback,
            context: default_context,
        }
    }
}

fn required(value: Option<usize>) -> Result<usize, BroadFailure> {
    let first = value.ok_or(BroadFailure::Missing)?;
    let second = value.ok_or_else(|| BroadFailure::Missing)?;

    Ok(first + second)
}
"#;

        assert_eq!(
            rules(source),
            [
                Rule::ContextErasingFailureConversion,
                Rule::ContextErasingFailureConversion,
                Rule::ContextErasingFailureConversion,
                Rule::ContextErasingFailureConversion,
            ]
        );
    }

    #[test]
    fn ordinary_functions_without_failure_inputs_may_construct_payloads() {
        let source = r#"
fn construct(request: CheckerRequestContext) -> BroadFailure {
    BroadFailure::Infrastructure {
        cause: request.failure(),
        context: request.context(),
    }
}

fn construct_with_resolver(
    resolver: &dyn Resolver<UpstreamError = C::UpstreamError>,
) -> BroadFailure {
    BroadFailure::Infrastructure {
        cause: resolver.failure(),
        context: resolver.context(),
    }
}
"#;

        assert_eq!(rules(source), []);
    }

    #[test]
    fn no_arg_closures_and_payload_free_match_variants_are_not_conversions() {
        let source = r#"
fn construct(kind: SpecificKind) -> BroadFailure {
    let deferred = || BroadFailure::Infrastructure {
        cause: fallback,
        context: default_context,
    };

    match kind {
        SpecificKind::Missing => BroadFailure::Infrastructure {
            cause: SpecificCause::Missing,
            context: default_context,
        },
    }
}
"#;

        assert_eq!(rules(source), []);
    }

    #[test]
    fn expression_exemptions_are_narrow_and_must_suppress_a_finding() {
        let valid = r#"
fn missing() -> BroadFailure {
    // rust-style: allow(context-erasing-failure-conversion, reason = "absence has no narrower leaf cause")
    BroadFailure::Missing
}
"#;

        let unused = r#"
fn preserved(error: LeafFailure, context: Context) -> BroadFailure {
    // rust-style: allow(context-erasing-failure-conversion, reason = "not needed")
    BroadFailure::Infrastructure {
        cause: error,
        context,
    }
}
"#;

        assert_eq!(rules(valid), []);
        assert_eq!(rules(unused), [Rule::UnusedExemption]);
    }

    #[test]
    fn malformed_markers_and_test_only_conversions_are_handled_deterministically() {
        let malformed = r#"
enum BroadFailure {
    // rust-style: broad-failure(reason = "invalid")
    Missing,
}
"#;

        let policy = Policy::from_sources(&[malformed]);
        let file = SourceFile::parse(malformed, Edition::Edition2024).tree();

        let diagnostics = check(Path::new("src/example.rs"), malformed, &file, &policy);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, Rule::InvalidFailureCategory);

        let test_marker = r#"
#[cfg(test)]
mod tests {
    enum TestFailure {
        // rust-style: broad-failure(extra)
        Missing,
    }
}
"#;

        let policy = Policy::from_sources(&[test_marker]);
        let file = SourceFile::parse(test_marker, Edition::Edition2024).tree();

        assert!(check(Path::new("src/example.rs"), test_marker, &file, &policy,).is_empty());

        let test_only = r#"
#[cfg(test)]
mod tests {
    fn ignored() -> BroadFailure {
        BroadFailure::Missing
    }
}
"#;

        assert_eq!(rules(test_only), []);

        let dedicated_test = r#"
fn ignored() -> BroadFailure {
    BroadFailure::Missing
}
"#;

        let policy = Policy::from_sources(&[CATEGORY, dedicated_test]);
        let file = SourceFile::parse(dedicated_test, Edition::Edition2024).tree();

        assert!(
            check(
                Path::new("tests/integration.rs"),
                dedicated_test,
                &file,
                &policy,
            )
            .is_empty()
        );
    }
}
